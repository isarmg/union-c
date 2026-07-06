//! ram 的认证和路径权限模块。
//!
//! 这个文件处理两类问题：
//! 1. “你是谁”：通过 Basic/Digest Authorization 头或临时 token 判断用户身份。
//! 2. “你能访问哪里”：通过 `AccessPaths` 判断某个路径是否可读、可写、仅可在索引中出现。
//!
//! ram 的 auth 规则格式类似：`user:pass@/public,/media:rw`。
//! 含义是：user 用户可以只读 `/public`，并读写 `/media`。

use crate::{args::Args, server::Response, utils::unix_now};

use anyhow::{anyhow, bail, Result};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use ed25519_dalek::{ed25519::signature::SignerMut, Signature, SigningKey};
use headers::HeaderValue;
use hyper::{header::WWW_AUTHENTICATE, Method};
use indexmap::IndexMap;
use lazy_static::lazy_static;
use md5::Context;
use sha2::{Digest, Sha256};
use sha_crypt::PasswordVerifier;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};
use uuid::Uuid;

const REALM: &str = "RAM";
const DIGEST_AUTH_TIMEOUT: u32 = 60 * 60 * 24 * 7; // 7 days
const TOKEN_EXPIRATION: u64 = 1000 * 60 * 60 * 24 * 3; // 3 days

lazy_static! {
    // Digest 认证里的 nonce 需要防伪造。这里用随机 UUID + 当前进程 id 做启动时盐值。
    // 服务重启后 nonce 会失效，这是可接受的安全边界。
    static ref NONCESTARTHASH: Context = {
        let mut h = Context::new();
        h.consume(Uuid::new_v4().as_bytes());
        h.consume(std::process::id().to_be_bytes());
        h
    };
}

/// 完整访问控制配置。
///
/// - `empty` 表示没有配置 auth，此时默认所有人都可以读写。
/// - `users` 保存具名用户及其路径权限。
/// - `anonymous` 保存匿名用户可访问的路径。
#[derive(Debug, Clone, PartialEq)]
pub struct AccessControl {
    empty: bool,
    use_hashed_password: bool,
    users: IndexMap<String, (String, AccessPaths)>,
    anonymous: Option<AccessPaths>,
}

impl Default for AccessControl {
    fn default() -> Self {
        AccessControl {
            empty: true,
            use_hashed_password: false,
            users: IndexMap::new(),
            anonymous: Some(AccessPaths::new(AccessPerm::ReadWrite)),
        }
    }
}

impl AccessControl {
    /// 根据原始 `--auth` 规则创建访问控制配置。
    pub fn new(raw_rules: &[&str]) -> Result<Self> {
        // 没有 auth 规则时，ram 采取“完全开放”的默认行为。
        if raw_rules.is_empty() {
            return Ok(Self::default());
        }
        let mut use_hashed_password = false;
        let mut annoy_paths = None;
        let mut account_paths_pairs = vec![];
        for rule in raw_rules {
            // 每条规则必须能拆成 account 和 paths 两部分，例如 user:pass@/dir:rw。
            let (account, paths) =
                split_account_paths(rule).ok_or_else(|| anyhow!("Invalid auth `{rule}`"))?;
            if account.is_empty() {
                // account 为空表示匿名规则，例如 @/public。
                if annoy_paths.is_some() {
                    bail!("Invalid auth, no duplicate anonymous rules");
                }
                annoy_paths = Some(paths)
            } else if let Some((user, pass)) = account.split_once(':') {
                if user.is_empty() || pass.is_empty() {
                    bail!("Invalid auth `{rule}`");
                }
                account_paths_pairs.push((user, pass, paths));
            }
        }
        let mut anonymous = None;
        if let Some(paths) = annoy_paths {
            // 匿名路径单独构建权限树。匿名规则通常用来开放公共目录。
            let mut access_paths = AccessPaths::default();
            access_paths
                .merge(paths)
                .ok_or_else(|| anyhow!("Invalid auth value `@{paths}"))?;
            anonymous = Some(access_paths);
        }
        let mut users = IndexMap::new();
        for (user, pass, paths) in account_paths_pairs.into_iter() {
            // 每个具名用户也会拥有一棵自己的路径权限树。
            let mut access_paths = AccessPaths::default();
            access_paths
                .merge(paths)
                .ok_or_else(|| anyhow!("Invalid auth value `{user}:{pass}@{paths}"))?;
            if let Some(anon_ap) = &anonymous {
                // 具名用户通常也应继承匿名可访问的公共路径。
                let orig_user = access_paths.clone();
                access_paths.absorb_anon(
                    anon_ap,
                    &orig_user,
                    AccessPerm::IndexOnly,
                    AccessPerm::IndexOnly,
                );
            }
            if pass.starts_with("$6$") {
                // 以 $6$ 开头时认为是 sha512-crypt 密码哈希，只能使用 Basic 认证校验。
                use_hashed_password = true;
            }
            users.insert(user.to_string(), (pass.to_string(), access_paths));
        }

        Ok(Self {
            empty: false,
            use_hashed_password,
            users,
            anonymous,
        })
    }

    /// 是否配置了具名用户。
    pub fn has_users(&self) -> bool {
        !self.users.is_empty()
    }

    /// 根据请求路径、HTTP 方法、Authorization 头和 token 判断访问权限。
    pub fn guard(
        &self,
        path: &str,
        method: &Method,
        authorization: Option<&HeaderValue>,
        token: Option<&String>,
        guard_options: bool,
    ) -> (Option<String>, Option<AccessPaths>) {
        // 返回值的含义：
        // - (None, Some(paths))：匿名访问通过；
        // - (Some(user), Some(paths))：具名用户访问通过；
        // - (Some(user), None)：用户身份正确，但权限不够；
        // - (None, None)：未认证或认证失败。
        if self.empty {
            return (None, Some(AccessPaths::new(AccessPerm::ReadWrite)));
        }

        if method == Method::GET {
            if let Some(token) = token {
                // token 只用于 GET，方便下载链接临时授权，不用于写操作。
                if let Ok((user, ap)) = self.verify_token(token, path) {
                    return (Some(user), ap.guard(path, method));
                }
            }
        }

        if let Some(authorization) = authorization {
            // 只要提供了 Authorization，就优先按具名用户校验。
            if let Some(user) = get_auth_user(authorization) {
                if let Some((pass, ap)) = self.users.get(&user) {
                    if method == Method::OPTIONS {
                        return (Some(user), Some(AccessPaths::new(AccessPerm::ReadOnly)));
                    }
                    if check_auth(authorization, method.as_str(), &user, pass).is_some() {
                        return (Some(user), ap.guard(path, method));
                    }
                }
            }

            return (None, None);
        }

        if !guard_options && method == Method::OPTIONS {
            return (None, Some(AccessPaths::new(AccessPerm::ReadOnly)));
        }

        if let Some(ap) = self.anonymous.as_ref() {
            // 没有登录信息时，最后尝试匿名权限。
            return (None, ap.guard(path, method));
        }

        (None, None)
    }

    /// 为用户生成绑定路径的临时访问 token。
    pub fn generate_token(&self, path: &str, user: &str) -> Result<String> {
        // token 把 “路径 + 过期时间 + 用户” 签名后编码成十六进制字符串。
        // 客户端拿着 token 可以在有效期内访问对应路径。
        let (pass, _) = self
            .users
            .get(user)
            .ok_or_else(|| anyhow!("Not found user '{user}'"))?;
        let exp = unix_now().as_millis() as u64 + TOKEN_EXPIRATION;
        let message = format!("{path}:{exp}");
        let mut signing_key = derive_secret_key(user, pass);
        let sig = signing_key.sign(message.as_bytes()).to_bytes();

        let mut raw = Vec::with_capacity(64 + 8 + user.len());
        raw.extend_from_slice(&sig);
        raw.extend_from_slice(&exp.to_be_bytes());
        raw.extend_from_slice(user.as_bytes());

        Ok(hex::encode(raw))
    }

    /// 校验 token，并返回 token 对应的用户名和权限树。
    fn verify_token<'a>(&'a self, token: &str, path: &str) -> Result<(String, &'a AccessPaths)> {
        // token 校验会检查三件事：格式正确、未过期、签名和当前用户密码能对应。
        let raw = hex::decode(token)?;

        if raw.len() < 72 {
            bail!("Invalid token");
        }

        let sig_bytes = &raw[..64];
        let exp_bytes = &raw[64..72];
        let user_bytes = &raw[72..];

        let exp = u64::from_be_bytes(exp_bytes.try_into()?);
        if unix_now().as_millis() as u64 > exp {
            bail!("Token expired");
        }

        let user = std::str::from_utf8(user_bytes)?;
        let (pass, ap) = self
            .users
            .get(user)
            .ok_or_else(|| anyhow!("Not found user '{user}'"))?;

        let sig = Signature::from_bytes(&<[u8; 64]>::try_from(sig_bytes)?);

        let message = format!("{path}:{exp}");
        derive_secret_key(user, pass).verify(message.as_bytes(), &sig)?;
        Ok((user.to_string(), ap))
    }
}

/// 路径权限树。
///
/// 初学者可以把它想象成一棵目录树：
/// - 根节点代表 `/`；
/// - 每个 children 代表下一级目录；
/// - 每个节点上保存这个路径的权限。
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct AccessPaths {
    perm: AccessPerm,
    children: IndexMap<String, AccessPaths>,
}

impl AccessPaths {
    /// 创建一棵只有根权限的权限树。
    pub fn new(perm: AccessPerm) -> Self {
        Self {
            perm,
            ..Default::default()
        }
    }

    /// 读取当前节点的权限。
    pub fn perm(&self) -> AccessPerm {
        self.perm
    }

    /// 设置当前节点权限。IndexOnly 不会覆盖已有可读/可写权限。
    pub fn set_perm(&mut self, perm: AccessPerm) {
        if !perm.indexonly() {
            self.perm = perm;
        }
    }

    /// 把 ram auth 规则里的路径字符串合并进权限树。
    pub fn merge(&mut self, paths: &str) -> Option<()> {
        // 把 `/a,/b:rw` 这种逗号分隔字符串逐项加入权限树。
        for item in paths.trim_matches(',').split(',') {
            let (path, perm) = match item.split_once(':') {
                None => (item, AccessPerm::ReadOnly),
                Some((path, "ro")) => (path, AccessPerm::ReadOnly),
                Some((path, "rw")) => (path, AccessPerm::ReadWrite),
                _ => return None,
            };
            self.add(path, perm);
        }
        Some(())
    }

    /// 检查某个请求路径和方法是否允许访问。
    pub fn guard(&self, path: &str, method: &Method) -> Option<Self> {
        // 先找到目标路径的有效权限；如果请求是写操作，则必须具备 ReadWrite。
        let target = self.find(path)?;
        if !is_readonly_method(method) && !target.perm().readwrite() {
            return None;
        }
        Some(target)
    }

    fn add(&mut self, path: &str, perm: AccessPerm) {
        // 空路径表示根目录 `/`；非空路径按 `/` 拆成多级节点。
        let path = path.trim_matches('/');
        if path.is_empty() {
            self.set_perm(perm);
        } else {
            let parts: Vec<&str> = path.split('/').collect();
            self.add_impl(&parts, perm);
        }
    }

    fn add_impl(&mut self, parts: &[&str], perm: AccessPerm) {
        if parts.is_empty() {
            self.perm = perm;
            return;
        }
        let child = self.children.entry(parts[0].to_string()).or_default();
        child.add_impl(&parts[1..], perm)
    }

    /// 将匿名权限合并进具名用户权限。
    ///
    /// 合并规则是“权限高者胜出”：如果匿名用户能读某个公共目录，登录用户也应该能读。
    /// `orig_user` 是合并前的用户权限快照，用来避免递归合并时把已经合并过的结果又算一遍。
    fn absorb_anon(
        &mut self,
        anon: &AccessPaths,
        orig_user: &AccessPaths,
        user_inherited: AccessPerm,
        anon_inherited: AccessPerm,
    ) {
        let anon_eff = if !anon.perm.indexonly() {
            anon.perm
        } else {
            anon_inherited
        };
        let orig_user_eff = if !orig_user.perm.indexonly() {
            orig_user.perm
        } else {
            user_inherited
        };

        let combined = std::cmp::max(anon_eff, orig_user_eff);
        if !combined.indexonly() && combined > self.perm {
            self.perm = combined;
        }

        let default_ap = AccessPaths::default();
        for (name, anon_child) in &anon.children {
            let orig_user_child = orig_user.children.get(name).unwrap_or(&default_ap);
            let user_child = self.children.entry(name.clone()).or_default();
            user_child.absorb_anon(anon_child, orig_user_child, orig_user_eff, anon_eff);
        }
    }

    pub fn find(&self, path: &str) -> Option<AccessPaths> {
        // 查询某个路径的有效权限。没有显式子节点时，会继承父目录权限。
        let parts: Vec<&str> = path
            .trim_matches('/')
            .split('/')
            .filter(|v| !v.is_empty())
            .collect();
        self.find_impl(&parts, self.perm)
    }

    fn find_impl(&self, parts: &[&str], perm: AccessPerm) -> Option<AccessPaths> {
        let perm = if !self.perm.indexonly() {
            self.perm
        } else {
            perm
        };
        if parts.is_empty() {
            if perm.indexonly() {
                return Some(self.clone());
            } else {
                return Some(AccessPaths::new(perm));
            }
        }
        let child = match self.children.get(parts[0]) {
            Some(v) => v,
            None => {
                if perm.indexonly() {
                    return None;
                } else {
                    return Some(AccessPaths::new(perm));
                }
            }
        };
        child.find_impl(&parts[1..], perm)
    }

    pub fn child_names(&self) -> Vec<&String> {
        self.children.keys().collect()
    }

    pub fn entry_paths(&self, base: &Path) -> Vec<PathBuf> {
        // 当用户只有 IndexOnly 权限时，需要列出它真正可进入的子路径。
        if !self.perm().indexonly() {
            return vec![base.to_path_buf()];
        }
        let mut output = vec![];
        self.entry_paths_impl(&mut output, base);
        output
    }

    fn entry_paths_impl(&self, output: &mut Vec<PathBuf>, base: &Path) {
        for (name, child) in self.children.iter() {
            let base = base.join(name);
            if child.perm().indexonly() {
                child.entry_paths_impl(output, &base);
            } else {
                output.push(base)
            }
        }
    }
}

/// 路径权限等级。枚举顺序也用于比较大小：ReadWrite > ReadOnly > IndexOnly。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum AccessPerm {
    #[default]
    IndexOnly,
    ReadOnly,
    ReadWrite,
}

impl AccessPerm {
    /// 是否是仅可出现在目录索引中的权限。
    pub fn indexonly(&self) -> bool {
        self == &AccessPerm::IndexOnly
    }

    /// 是否是可读写权限。
    pub fn readwrite(&self) -> bool {
        self == &AccessPerm::ReadWrite
    }
}

/// 给 401 响应添加 WWW-Authenticate 头，提示客户端如何认证。
pub fn www_authenticate(res: &mut Response, args: &Args) -> Result<()> {
    // 返回 401 时要带 WWW-Authenticate，浏览器才知道应该弹出登录框或发送认证信息。
    if args.auth.use_hashed_password {
        let basic = HeaderValue::from_str(&format!("Basic realm=\"{REALM}\""))?;
        res.headers_mut().insert(WWW_AUTHENTICATE, basic);
    } else {
        let nonce = create_nonce()?;
        let digest = HeaderValue::from_str(&format!(
            "Digest realm=\"{REALM}\", nonce=\"{nonce}\", qop=\"auth\""
        ))?;
        let basic = HeaderValue::from_str(&format!("Basic realm=\"{REALM}\""))?;
        res.headers_mut().append(WWW_AUTHENTICATE, digest);
        res.headers_mut().append(WWW_AUTHENTICATE, basic);
    }
    Ok(())
}

/// 从 Authorization 头中提取用户名，主要用于日志显示。
pub fn get_auth_user(authorization: &HeaderValue) -> Option<String> {
    // 这里只解析出用户名，不验证密码。密码校验在 check_auth 中完成。
    if let Some(value) = strip_prefix(authorization.as_bytes(), b"Basic ") {
        let value: Vec<u8> = STANDARD.decode(value).ok()?;
        let parts: Vec<&str> = std::str::from_utf8(&value).ok()?.split(':').collect();
        Some(parts[0].to_string())
    } else if let Some(value) = strip_prefix(authorization.as_bytes(), b"Digest ") {
        let digest_map = to_headermap(value).ok()?;
        let username = digest_map.get(b"username".as_ref())?;
        std::str::from_utf8(username).map(|v| v.to_string()).ok()
    } else {
        None
    }
}

/// 校验 Basic 或 Digest Authorization 是否正确。
pub fn check_auth(
    authorization: &HeaderValue,
    method: &str,
    auth_user: &str,
    auth_pass: &str,
) -> Option<()> {
    // 支持 Basic 和 Digest 两种 HTTP 认证。
    // Basic 是 base64(user:pass)，Digest 则不会直接把密码放到请求里。
    if let Some(value) = strip_prefix(authorization.as_bytes(), b"Basic ") {
        let value: Vec<u8> = STANDARD.decode(value).ok()?;
        let (user, pass) = std::str::from_utf8(&value).ok()?.split_once(':')?;

        if user != auth_user {
            return None;
        }

        if auth_pass.starts_with("$6$") {
            // sha512-crypt 哈希密码只能通过验证函数比对，不能直接字符串比较。
            if sha_crypt::ShaCrypt::SHA512
                .verify_password(pass.as_bytes(), auth_pass)
                .is_ok()
            {
                return Some(());
            }
        } else if pass == auth_pass {
            return Some(());
        }

        None
    } else if let Some(value) = strip_prefix(authorization.as_bytes(), b"Digest ") {
        // Digest 认证需要按 RFC 规则重新计算 response，再和客户端给出的 response 比较。
        let digest_map = to_headermap(value).ok()?;
        if let (Some(username), Some(nonce), Some(user_response)) = (
            digest_map
                .get(b"username".as_ref())
                .and_then(|b| std::str::from_utf8(b).ok()),
            digest_map.get(b"nonce".as_ref()),
            digest_map.get(b"response".as_ref()),
        ) {
            match validate_nonce(nonce) {
                Ok(true) => {}
                _ => return None,
            }
            if auth_user != username {
                return None;
            }

            let mut h = Context::new();
            h.consume(format!("{auth_user}:{REALM}:{auth_pass}").as_bytes());
            let auth_pass = format!("{:x}", h.finalize());

            let mut ha = Context::new();
            ha.consume(method);
            ha.consume(b":");
            if let Some(uri) = digest_map.get(b"uri".as_ref()) {
                ha.consume(uri);
            }
            let ha = format!("{:x}", ha.finalize());
            let mut correct_response = None;
            if let Some(qop) = digest_map.get(b"qop".as_ref()) {
                if qop == &b"auth".as_ref() || qop == &b"auth-int".as_ref() {
                    correct_response = Some({
                        let mut c = Context::new();
                        c.consume(&auth_pass);
                        c.consume(b":");
                        c.consume(nonce);
                        c.consume(b":");
                        if let Some(nc) = digest_map.get(b"nc".as_ref()) {
                            c.consume(nc);
                        }
                        c.consume(b":");
                        if let Some(cnonce) = digest_map.get(b"cnonce".as_ref()) {
                            c.consume(cnonce);
                        }
                        c.consume(b":");
                        c.consume(qop);
                        c.consume(b":");
                        c.consume(&*ha);
                        format!("{:x}", c.finalize())
                    });
                }
            }
            let correct_response = match correct_response {
                Some(r) => r,
                None => {
                    let mut c = Context::new();
                    c.consume(&auth_pass);
                    c.consume(b":");
                    c.consume(nonce);
                    c.consume(b":");
                    c.consume(&*ha);
                    format!("{:x}", c.finalize())
                }
            };
            if correct_response.as_bytes() == *user_response {
                return Some(());
            }
        }
        None
    } else {
        None
    }
}

/// 根据用户名和密码派生 token 签名密钥。
fn derive_secret_key(user: &str, pass: &str) -> SigningKey {
    // token 签名密钥由用户名和密码派生。密码变了，旧 token 也会自然失效。
    let mut hasher = Sha256::new();
    hasher.update(format!("{user}:{pass}").as_bytes());
    let hash = hasher.finalize();
    SigningKey::from_bytes(&hash.into())
}

/// 检查 Digest nonce 是否仍然有效。
///
/// nonce 是服务端发给客户端的一次性挑战值，里面包含时间戳和服务端签名。
/// 如果格式不对、签名不对或过期，就不能继续认证。
fn validate_nonce(nonce: &[u8]) -> Result<bool> {
    if nonce.len() != 34 {
        bail!("invalid nonce");
    }
    //parse hex
    if let Ok(n) = std::str::from_utf8(nonce) {
        //get time
        if let Ok(secs_nonce) = u32::from_str_radix(&n[..8], 16) {
            //check time
            let now = unix_now();
            let secs_now = now.as_secs() as u32;

            if let Some(dur) = secs_now.checked_sub(secs_nonce) {
                //check hash
                let mut h = NONCESTARTHASH.clone();
                h.consume(secs_nonce.to_be_bytes());
                let h = format!("{:x}", h.finalize());
                if h[..26] == n[8..34] {
                    return Ok(dur < DIGEST_AUTH_TIMEOUT);
                }
            }
        }
    }
    bail!("invalid nonce");
}

/// 判断 HTTP 方法是否只读。
fn is_readonly_method(method: &Method) -> bool {
    // WebDAV 有一些非标准但只读的方法，也要纳入只读判断。
    method == Method::GET
        || method == Method::OPTIONS
        || method == Method::HEAD
        || method.as_str() == "PROPFIND"
        || method.as_str() == "CHECKAUTH"
        || method.as_str() == "LOGOUT"
}

fn strip_prefix<'a>(search: &'a [u8], prefix: &[u8]) -> Option<&'a [u8]> {
    let l = prefix.len();
    if search.len() < l {
        return None;
    }
    if &search[..l] == prefix {
        Some(&search[l..])
    } else {
        None
    }
}

fn to_headermap(header: &[u8]) -> Result<HashMap<&[u8], &[u8]>, ()> {
    // Digest 头形如 key="value", key2=value2。这里做一个轻量解析器。
    let mut sep = Vec::new();
    let mut assign = Vec::new();
    let mut i: usize = 0;
    let mut esc = false;
    for c in header {
        match (c, esc) {
            (b'=', false) => assign.push(i),
            (b',', false) => sep.push(i),
            (b'"', false) => esc = true,
            (b'"', true) => esc = false,
            _ => {}
        }
        i += 1;
    }
    sep.push(i);

    i = 0;
    let mut ret = HashMap::new();
    for (&k, &a) in sep.iter().zip(assign.iter()) {
        while header[i] == b' ' {
            i += 1;
        }
        if a <= i || k <= 1 + a {
            //keys and values must contain one char
            return Err(());
        }
        let key = &header[i..a];
        let val = if header[1 + a] == b'"' && header[k - 1] == b'"' {
            //escaped
            &header[2 + a..k - 1]
        } else {
            //not escaped
            &header[1 + a..k]
        };
        i = 1 + k;
        ret.insert(key, val);
    }
    Ok(ret)
}

/// 创建 Digest 认证用的 nonce。
fn create_nonce() -> Result<String> {
    // nonce 前 8 位是时间戳，后面是带启动盐值的哈希片段。
    let now = unix_now();
    let secs = now.as_secs() as u32;
    let mut h = NONCESTARTHASH.clone();
    h.consume(secs.to_be_bytes());

    let n = format!("{:08x}{:032x}", secs, h.finalize());
    Ok(n[..34].to_string())
}

/// 把 `account@paths` 拆成账号部分和路径部分。
fn split_account_paths(s: &str) -> Option<(&str, &str)> {
    // 按第一个 "@/” 分割，确保密码里普通 @ 字符不会误伤。
    let i = s.find("@/")?;
    Some((&s[0..i], &s[i + 1..]))
}
