//! ram 的参数解析模块。
//!
//! 这个文件负责三件事：
//! 1. 用 clap 声明命令行参数，例如 `--bind`、`--port`、`--auth`。
//! 2. 用 serde 支持 YAML 配置文件。
//! 3. 把默认值、配置文件和命令行参数合并成最终的 `Args`。

use anyhow::{bail, Context, Result};
use async_deflate_zip::Compression;
use clap::builder::{PossibleValue, PossibleValuesParser};
use clap::{value_parser, Arg, ArgAction, ArgMatches, Command, ValueEnum};
use clap_complete::{generate, Generator, Shell};
use serde::{Deserialize, Deserializer};
use smart_default::SmartDefault;
use std::env;
use std::net::IpAddr;
use std::path::{Path, PathBuf};

use crate::auth::AccessControl;
use crate::http_logger::HttpLogger;
use crate::utils::{encode_uri, is_ipv6_available};

/// 构建命令行解析器。
///
/// clap 通过这里声明的参数生成 `--help`、环境变量绑定和命令行解析规则。
pub fn build_cli() -> Command {
    // clap 的 Command 只描述“这个程序支持哪些参数”，真正读取参数在 main.rs 的 get_matches。
    let app = Command::new(env!("CARGO_CRATE_NAME"))
        .version(env!("CARGO_PKG_VERSION"))
        .author(env!("CARGO_PKG_AUTHORS"))
        .about(concat!(
            env!("CARGO_PKG_DESCRIPTION"),
            " - ",
            env!("CARGO_PKG_REPOSITORY")
        ))
        .arg(
            Arg::new("serve-path")
                .env("RAM_SERVE_PATH")
				.hide_env(true)
                .value_parser(value_parser!(PathBuf))
                .help("Specific path to serve [default: .]"),
        )
        .arg(
            Arg::new("config")
                .env("RAM_CONFIG")
				.hide_env(true)
                .short('c')
                .long("config")
                .value_parser(value_parser!(PathBuf))
                .help("Specify configuration file")
                .value_name("file"),
        )
        .arg(
            Arg::new("bind")
                .env("RAM_BIND")
				.hide_env(true)
                .short('b')
                .long("bind")
                .help("Specify bind address or unix socket")
                .action(ArgAction::Append)
                .value_delimiter(',')
                .value_name("addrs"),
        )
        .arg(
            Arg::new("port")
                .env("RAM_PORT")
				.hide_env(true)
                .short('p')
                .long("port")
                .value_parser(value_parser!(u16))
                .help("Specify port to listen on [default: 5000]")
                .value_name("port"),
        )
        .arg(
            Arg::new("path-prefix")
                .env("RAM_PATH_PREFIX")
				.hide_env(true)
                .long("path-prefix")
                .value_name("path")
                .help("Specify a path prefix"),
        )
        .arg(
            Arg::new("hidden")
                .env("RAM_HIDDEN")
				.hide_env(true)
                .long("hidden")
                .action(ArgAction::Append)
                .value_delimiter(',')
                .help("Hide paths from directory listings, e.g. tmp,*.log,*.lock")
                .value_name("value"),
        )
        .arg(
            Arg::new("auth")
                .env("RAM_AUTH")
				.hide_env(true)
                .short('a')
                .long("auth")
                .help("Add auth roles, e.g. user:pass@/dir1:rw,/dir2")
                .action(ArgAction::Append)
                .value_name("rules"),
        )
        .arg(
            Arg::new("auth-state-file")
                .env("RAM_AUTH_STATE_FILE")
                .hide_env(true)
                .long("auth-state-file")
                .value_parser(value_parser!(PathBuf))
                .help("Persist runtime auth updates from the protected management API")
                .value_name("file"),
        )
        .arg(
            Arg::new("auth-method")
                .hide(true)
                .env("RAM_AUTH_METHOD")
				.hide_env(true)
                .long("auth-method")
                .help("Select auth method")
                .value_parser(PossibleValuesParser::new(["basic", "digest"]))
                .default_value("digest")
                .value_name("value"),
        )
        .arg(
            Arg::new("allow-all")
                .env("RAM_ALLOW_ALL")
				.hide_env(true)
                .short('A')
                .long("allow-all")
                .action(ArgAction::SetTrue)
                .help("Allow all operations"),
        )
        .arg(
            Arg::new("allow-upload")
                .env("RAM_ALLOW_UPLOAD")
				.hide_env(true)
                .long("allow-upload")
                .action(ArgAction::SetTrue)
                .help("Allow upload files/folders"),
        )
        .arg(
            Arg::new("allow-delete")
                .env("RAM_ALLOW_DELETE")
				.hide_env(true)
                .long("allow-delete")
                .action(ArgAction::SetTrue)
                .help("Allow delete files/folders"),
        )
        .arg(
            Arg::new("allow-search")
                .env("RAM_ALLOW_SEARCH")
				.hide_env(true)
                .long("allow-search")
                .action(ArgAction::SetTrue)
                .help("Allow search files/folders"),
        )
        .arg(
            Arg::new("allow-symlink")
                .env("RAM_ALLOW_SYMLINK")
				.hide_env(true)
                .long("allow-symlink")
                .action(ArgAction::SetTrue)
                .help("Allow symlink to files/folders outside root directory"),
        )
        .arg(
            Arg::new("allow-archive")
                .env("RAM_ALLOW_ARCHIVE")
				.hide_env(true)
                .long("allow-archive")
                .action(ArgAction::SetTrue)
                .help("Allow download folders as archive file"),
        )
        .arg(
            Arg::new("allow-hash")
                .env("RAM_ALLOW_HASH")
                .hide_env(true)
                .long("allow-hash")
                .action(ArgAction::SetTrue)
                .help("Allow ?hash query to get file sha256 hash"),
        )
        .arg(
            Arg::new("enable-cors")
                .env("RAM_ENABLE_CORS")
				.hide_env(true)
                .long("enable-cors")
                .action(ArgAction::SetTrue)
                .help("Enable CORS, sets `Access-Control-Allow-Origin: *`"),
        )
        .arg(
            Arg::new("render-index")
                .env("RAM_RENDER_INDEX")
				.hide_env(true)
                .long("render-index")
                .action(ArgAction::SetTrue)
                .help("Serve index.html when requesting a directory, returns 404 if not found index.html"),
        )
        .arg(
            Arg::new("render-try-index")
                .env("RAM_RENDER_TRY_INDEX")
				.hide_env(true)
                .long("render-try-index")
                .action(ArgAction::SetTrue)
                .help("Serve index.html when requesting a directory, returns directory listing if not found index.html"),
        )
        .arg(
            Arg::new("render-spa")
                .env("RAM_RENDER_SPA")
				.hide_env(true)
                .long("render-spa")
                .action(ArgAction::SetTrue)
                .help("Serve SPA(Single Page Application)"),
        )
        .arg(
            Arg::new("assets")
                .env("RAM_ASSETS")
				.hide_env(true)
                .long("assets")
                .help("Set the path to the assets directory for overriding the built-in assets")
                .value_parser(value_parser!(PathBuf))
                .value_name("path")
        )
        .arg(
            Arg::new("log-format")
                .env("RAM_LOG_FORMAT")
                .hide_env(true)
                .long("log-format")
                .value_name("format")
                .help("Customize http log format"),
        )
        .arg(
            Arg::new("log-file")
                .env("RAM_LOG_FILE")
                .hide_env(true)
                .long("log-file")
                .value_name("file")
                .value_parser(value_parser!(PathBuf))
                .help("Specify the file to save logs to, other than stdout/stderr"),
        )
        .arg(
            Arg::new("compress")
                .env("RAM_COMPRESS")
                .hide_env(true)
                .value_parser(clap::builder::EnumValueParser::<Compress>::new())
                .long("compress")
                .value_name("level")
                .help("Set zip compress level [default: low]")
        )
        .arg(
            Arg::new("completions")
                .long("completions")
                .value_name("shell")
                .value_parser(value_parser!(Shell))
                .help("Print shell completion script for <shell>"),
        );

    #[cfg(feature = "tls")]
    let app = app
        .arg(
            Arg::new("tls-cert")
                .env("RAM_TLS_CERT")
                .hide_env(true)
                .long("tls-cert")
                .value_name("path")
                .value_parser(value_parser!(PathBuf))
                .help("Path to an SSL/TLS certificate to serve with HTTPS"),
        )
        .arg(
            Arg::new("tls-key")
                .env("RAM_TLS_KEY")
                .hide_env(true)
                .long("tls-key")
                .value_name("path")
                .value_parser(value_parser!(PathBuf))
                .help("Path to the SSL/TLS certificate's private key"),
        );

    app
}

/// 输出 shell 自动补全脚本。
///
/// 例如 `ram --completions bash` 会把 bash 补全脚本打印到 stdout。
pub fn print_completions<G: Generator>(gen: G, cmd: &mut Command) {
    // 生成 bash/zsh/fish 等 shell 的自动补全脚本。
    generate(gen, cmd, cmd.get_name().to_string(), &mut std::io::stdout());
}

/// ram 运行时的完整配置。
///
/// 字段来源有三类：
/// - `SmartDefault` 和下面的 default_* 函数提供默认值；
/// - YAML 配置文件通过 serde 反序列化进来；
/// - 命令行参数最后覆盖配置文件中的同名项。
#[derive(Debug, Deserialize, SmartDefault, PartialEq)]
#[serde(default)]
#[serde(rename_all = "kebab-case")]
pub struct Args {
    /// 要对外提供访问的本地路径。可以是目录，也可以是单个文件。
    #[serde(default = "default_serve_path")]
    #[default(default_serve_path())]
    pub serve_path: PathBuf,
    /// 监听地址。常见值是 0.0.0.0、127.0.0.1、::。
    #[serde(deserialize_with = "deserialize_bind_addrs")]
    #[serde(rename = "bind")]
    #[serde(default = "default_addrs")]
    #[default(default_addrs())]
    pub addrs: Vec<BindAddr>,
    /// HTTP 服务端口。
    #[serde(default = "default_port")]
    #[default(default_port())]
    pub port: u16,
    /// serve_path 是否是单文件。这个字段运行时计算，不从配置文件读取。
    #[serde(skip)]
    pub path_is_file: bool,
    /// URL 前缀，例如 /files。它让 ram 可以挂在反向代理的子路径下。
    pub path_prefix: String,
    /// 已经编码后的 URL 前缀，内部拼接链接时使用。
    #[serde(skip)]
    pub uri_prefix: String,
    /// 目录列表中要隐藏的文件名或 glob 模式。
    #[serde(deserialize_with = "deserialize_string_or_vec")]
    pub hidden: Vec<String>,
    /// 访问控制规则，来自 --auth 或配置文件 auth。
    #[serde(deserialize_with = "deserialize_access_control")]
    pub auth: AccessControl,
    /// 管理 API 热更新认证后写入的私有状态文件。
    pub auth_state_file: Option<PathBuf>,
    /// 是否允许所有操作。开启后等价于放开上传、删除、搜索等能力。
    pub allow_all: bool,
    /// 是否允许客户端上传文件。
    pub allow_upload: bool,
    /// 是否允许客户端删除文件或目录。
    pub allow_delete: bool,
    /// 是否允许搜索目录内容。
    pub allow_search: bool,
    /// 是否允许访问符号链接指向的目标。
    pub allow_symlink: bool,
    /// 是否允许把目录打包成 zip 下载。
    pub allow_archive: bool,
    /// 是否允许计算文件 hash。
    pub allow_hash: bool,
    /// 是否把目录渲染成内置前端页面。
    pub render_index: bool,
    /// 是否按 SPA 模式处理找不到的路径。
    pub render_spa: bool,
    /// 是否在目录中尝试返回 index.html。
    pub render_try_index: bool,
    /// 是否给响应添加 CORS 头。
    pub enable_cors: bool,
    /// 自定义前端静态资源目录。
    pub assets: Option<PathBuf>,
    /// 自定义错误页面路径。
    pub error_page: Option<PathBuf>,
    #[serde(deserialize_with = "deserialize_log_http")]
    #[serde(rename = "log-format")]
    pub http_logger: HttpLogger,
    /// HTTP 访问日志输出文件。
    pub log_file: Option<PathBuf>,
    /// 响应压缩等级。
    pub compress: Compress,
    /// TLS 证书路径。
    pub tls_cert: Option<PathBuf>,
    /// TLS 私钥路径。
    pub tls_key: Option<PathBuf>,
}

impl Args {
    /// 解析命令行参数。
    ///
    /// 合并顺序很重要：
    /// 1. 先创建默认配置；
    /// 2. 如果传了配置文件，用配置文件覆盖默认值；
    /// 3. 如果命令行传了具体参数，再用命令行覆盖配置文件。
    pub fn parse(matches: ArgMatches) -> Result<Args> {
        let mut args = Self::default();

        if let Some(config_path) = matches.get_one::<PathBuf>("config") {
            // 配置文件使用 YAML 格式，字段名采用 kebab-case，例如 path-prefix。
            let contents = std::fs::read_to_string(config_path)
                .with_context(|| format!("Failed to read config at {}", config_path.display()))?;
            args = serde_yaml::from_str(&contents)
                .with_context(|| format!("Failed to load config at {}", config_path.display()))?;
        }

        if let Some(path) = matches.get_one::<PathBuf>("serve-path") {
            args.serve_path.clone_from(path)
        }

        args.serve_path = Self::sanitize_path(args.serve_path)?;

        // 下面每个 if let 都是在判断“命令行是否显式传了这个参数”。
        // 如果没传，就保留配置文件或默认值。
        if let Some(port) = matches.get_one::<u16>("port") {
            args.port = *port
        }

        if let Some(addrs) = matches.get_many::<String>("bind") {
            let addrs: Vec<_> = addrs.map(|v| v.as_str()).collect();
            args.addrs = BindAddr::parse_addrs(&addrs)?;
        }

        args.path_is_file = args.serve_path.metadata()?.is_file();
        if let Some(path_prefix) = matches.get_one::<String>("path-prefix") {
            args.path_prefix.clone_from(path_prefix)
        }
        // 内部只保存不带前后斜杠的前缀，避免后续拼 URL 时出现 //。
        args.path_prefix = args.path_prefix.trim_matches('/').to_string();

        args.uri_prefix = if args.path_prefix.is_empty() {
            "/".to_owned()
        } else {
            format!("/{}/", &encode_uri(&args.path_prefix))
        };

        if let Some(hidden) = matches.get_many::<String>("hidden") {
            args.hidden = hidden.cloned().collect();
        } else {
            // 配置文件里的 hidden 可能是 ["a,b"] 这种字符串，需要再次按逗号拆开。
            let mut hidden = vec![];
            std::mem::swap(&mut args.hidden, &mut hidden);
            args.hidden = hidden
                .into_iter()
                .flat_map(|v| v.split(',').map(|v| v.to_string()).collect::<Vec<String>>())
                .collect();
        }

        if !args.enable_cors {
            args.enable_cors = matches.get_flag("enable-cors");
        }

        if let Some(rules) = matches.get_many::<String>("auth") {
            let rules: Vec<_> = rules.map(|v| v.as_str()).collect();
            args.auth = AccessControl::new(&rules)?;
        }
        if let Some(path) = matches.get_one::<PathBuf>("auth-state-file") {
            args.auth_state_file = Some(path.clone());
        }
        if let Some(path) = &args.auth_state_file {
            if path.is_file() {
                let content = std::fs::read(path)
                    .with_context(|| format!("Failed to read auth state at {}", path.display()))?;
                let rules: Vec<String> = serde_json::from_slice(&content)
                    .with_context(|| format!("Failed to parse auth state at {}", path.display()))?;
                let refs = rules.iter().map(String::as_str).collect::<Vec<_>>();
                args.auth = AccessControl::new(&refs)?;
            }
        }

        if !args.allow_all {
            args.allow_all = matches.get_flag("allow-all");
        }

        let allow_all = args.allow_all;

        // allow-all 是总开关。它打开后，上传/删除/搜索/压缩下载等能力全部开启。
        if !args.allow_upload {
            args.allow_upload = allow_all || matches.get_flag("allow-upload");
        }
        if !args.allow_delete {
            args.allow_delete = allow_all || matches.get_flag("allow-delete");
        }
        if !args.allow_search {
            args.allow_search = allow_all || matches.get_flag("allow-search");
        }
        if !args.allow_symlink {
            args.allow_symlink = allow_all || matches.get_flag("allow-symlink");
        }
        if !args.allow_hash {
            args.allow_hash = allow_all || matches.get_flag("allow-hash");
        }
        if !args.allow_archive {
            args.allow_archive = allow_all || matches.get_flag("allow-archive");
        }
        if !args.render_index {
            args.render_index = matches.get_flag("render-index");
        }

        if !args.render_try_index {
            args.render_try_index = matches.get_flag("render-try-index");
        }

        if !args.render_spa {
            args.render_spa = matches.get_flag("render-spa");
        }

        if let Some(assets_path) = matches.get_one::<PathBuf>("assets") {
            args.assets = Some(assets_path.clone());
        }

        if let Some(assets_path) = &args.assets {
            // 自定义资源目录必须包含 index.html，否则前端页面无法启动。
            args.assets = Some(Args::sanitize_assets_path(assets_path)?);
        }

        if let Some(assets_path) = &args.assets {
            // 如果自定义资源目录里有 404.html，就用它作为错误页。
            let p = assets_path.join("404.html");
            if p.exists() {
                args.error_page = Some(p);
            }
        }

        if let Some(log_format) = matches.get_one::<String>("log-format") {
            args.http_logger = log_format.parse()?;
        }

        if let Some(log_file) = matches.get_one::<PathBuf>("log-file") {
            args.log_file = Some(log_file.clone());
        }

        if let Some(compress) = matches.get_one::<Compress>("compress") {
            args.compress = *compress;
        }

        #[cfg(feature = "tls")]
        {
            if let Some(tls_cert) = matches.get_one::<PathBuf>("tls-cert") {
                args.tls_cert = Some(tls_cert.clone())
            }

            if let Some(tls_key) = matches.get_one::<PathBuf>("tls-key") {
                args.tls_key = Some(tls_key.clone())
            }

            match (&args.tls_cert, &args.tls_key) {
                (Some(_), Some(_)) => {}
                (Some(_), _) => bail!("No tls-key set"),
                (_, Some(_)) => bail!("No tls-cert set"),
                (None, None) => {}
            }
        }
        #[cfg(not(feature = "tls"))]
        {
            args.tls_cert = None;
            args.tls_key = None;
        }

        Ok(args)
    }

    fn sanitize_path<P: AsRef<Path>>(path: P) -> Result<PathBuf> {
        let path = path.as_ref();
        if !path.exists() {
            bail!("Path `{}` doesn't exist", path.display());
        }

        // canonicalize 会把相对路径转成绝对路径，并解析 ..、符号链接等。
        // 这样后续安全检查可以基于标准化后的路径判断。
        env::current_dir()
            .and_then(|mut p| {
                p.push(path); // If path is absolute, it replaces the current path.
                std::fs::canonicalize(p)
            })
            .with_context(|| format!("Failed to access path `{}`", path.display()))
    }

    fn sanitize_assets_path<P: AsRef<Path>>(path: P) -> Result<PathBuf> {
        let path = Self::sanitize_path(path)?;
        if !path.join("index.html").exists() {
            bail!("Path `{}` doesn't contains index.html", path.display());
        }
        Ok(path)
    }
}

/// ram 可以绑定的地址。
///
/// Linux 下可绑定 IP 地址或 Unix socket 文件路径。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum BindAddr {
    IpAddr(IpAddr),
    SocketPath(String),
}

impl BindAddr {
    fn parse_addrs(addrs: &[&str]) -> Result<Vec<Self>> {
        // --bind 支持传多个值，也支持逗号分隔；这里统一解析成 BindAddr 列表。
        let mut bind_addrs = vec![];
        for addr in addrs {
            match addr.parse::<IpAddr>() {
                Ok(v) => {
                    bind_addrs.push(BindAddr::IpAddr(v));
                }
                Err(_) => {
                    bind_addrs.push(BindAddr::SocketPath(addr.to_string()));
                }
            }
        }
        Ok(bind_addrs)
    }
}

/// 下载目录为 zip 时的压缩等级。
#[derive(Debug, Clone, Copy, PartialEq, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Compress {
    None,
    #[default]
    Low,
    Medium,
    High,
}

impl ValueEnum for Compress {
    fn value_variants<'a>() -> &'a [Self] {
        &[Self::None, Self::Low, Self::Medium, Self::High]
    }

    fn to_possible_value(&self) -> Option<clap::builder::PossibleValue> {
        Some(match self {
            Compress::None => PossibleValue::new("none"),
            Compress::Low => PossibleValue::new("low"),
            Compress::Medium => PossibleValue::new("medium"),
            Compress::High => PossibleValue::new("high"),
        })
    }
}

impl Compress {
    pub fn to_compression(self) -> Compression {
        // 这里把命令行枚举转换成 async_deflate_zip 库需要的压缩参数。
        match self {
            Compress::None => Compression::none(),
            Compress::Low => Compression::fast(),
            Compress::Medium => Compression::default(),
            Compress::High => Compression::best(),
        }
    }
}

fn deserialize_bind_addrs<'de, D>(deserializer: D) -> Result<Vec<BindAddr>, D::Error>
where
    D: Deserializer<'de>,
{
    // 配置文件里 bind 既可以写成字符串，也可以写成数组；Visitor 用来兼容两种形式。
    struct StringOrVec;

    impl<'de> serde::de::Visitor<'de> for StringOrVec {
        type Value = Vec<BindAddr>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("string or list of strings")
        }

        fn visit_str<E>(self, s: &str) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            BindAddr::parse_addrs(&[s]).map_err(serde::de::Error::custom)
        }

        fn visit_seq<S>(self, seq: S) -> Result<Self::Value, S::Error>
        where
            S: serde::de::SeqAccess<'de>,
        {
            let addrs: Vec<&'de str> =
                Deserialize::deserialize(serde::de::value::SeqAccessDeserializer::new(seq))?;
            BindAddr::parse_addrs(&addrs).map_err(serde::de::Error::custom)
        }
    }

    deserializer.deserialize_any(StringOrVec)
}

fn deserialize_string_or_vec<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: Deserializer<'de>,
{
    // hidden 等字段也允许“单个字符串”或“字符串数组”两种写法。
    struct StringOrVec;

    impl<'de> serde::de::Visitor<'de> for StringOrVec {
        type Value = Vec<String>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("string or list of strings")
        }

        fn visit_str<E>(self, s: &str) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(vec![s.to_owned()])
        }

        fn visit_seq<S>(self, seq: S) -> Result<Self::Value, S::Error>
        where
            S: serde::de::SeqAccess<'de>,
        {
            Deserialize::deserialize(serde::de::value::SeqAccessDeserializer::new(seq))
        }
    }

    deserializer.deserialize_any(StringOrVec)
}

fn deserialize_access_control<'de, D>(deserializer: D) -> Result<AccessControl, D::Error>
where
    D: Deserializer<'de>,
{
    // 配置文件中的 auth 是字符串数组，解析后变成 AccessControl 权限树。
    let rules: Vec<&str> = Vec::deserialize(deserializer)?;
    AccessControl::new(&rules).map_err(serde::de::Error::custom)
}

fn deserialize_log_http<'de, D>(deserializer: D) -> Result<HttpLogger, D::Error>
where
    D: Deserializer<'de>,
{
    let value: String = Deserialize::deserialize(deserializer)?;
    value.parse().map_err(serde::de::Error::custom)
}

fn default_serve_path() -> PathBuf {
    PathBuf::from(".")
}

fn default_addrs() -> Vec<BindAddr> {
    // 默认监听所有 IPv4 地址；如果系统支持 IPv6，也同时监听 ::。
    let addrs = if is_ipv6_available() {
        ["0.0.0.0", "::"].as_slice()
    } else {
        ["0.0.0.0"].as_slice()
    };
    BindAddr::parse_addrs(addrs).unwrap()
}

fn default_port() -> u16 {
    5000
}

#[cfg(test)]
mod tests {
    use super::*;

    use assert_fs::prelude::*;

    #[test]
    fn test_default() {
        let cli = build_cli();
        let matches = cli.try_get_matches_from(vec![""]).unwrap();
        let args = Args::parse(matches).unwrap();
        let cwd = Args::sanitize_path(std::env::current_dir().unwrap()).unwrap();
        assert_eq!(args.serve_path, cwd);
        assert_eq!(args.port, default_port());
        assert_eq!(args.addrs, default_addrs());
    }

    #[test]
    fn test_args_from_cli1() {
        let tmpdir = assert_fs::TempDir::new().unwrap();
        let cli = build_cli();
        let matches = cli
            .try_get_matches_from(vec![
                "",
                "--hidden",
                "tmp,*.log,*.lock",
                &tmpdir.to_string_lossy(),
            ])
            .unwrap();
        let args = Args::parse(matches).unwrap();
        assert_eq!(args.serve_path, Args::sanitize_path(&tmpdir).unwrap());
        assert_eq!(args.hidden, ["tmp", "*.log", "*.lock"]);
    }

    #[test]
    fn test_args_from_cli2() {
        let cli = build_cli();
        let matches = cli
            .try_get_matches_from(vec![
                "", "--hidden", "tmp", "--hidden", "*.log", "--hidden", "*.lock",
            ])
            .unwrap();
        let args = Args::parse(matches).unwrap();
        assert_eq!(args.hidden, ["tmp", "*.log", "*.lock"]);
    }

    #[test]
    fn test_args_from_empty_config_file() {
        let tmpdir = assert_fs::TempDir::new().unwrap();
        let config_file = tmpdir.child("config.yaml");
        config_file.write_str("").unwrap();

        let cli = build_cli();
        let matches = cli
            .try_get_matches_from(vec!["", "-c", &config_file.to_string_lossy()])
            .unwrap();
        let args = Args::parse(matches).unwrap();
        let cwd = Args::sanitize_path(std::env::current_dir().unwrap()).unwrap();
        assert_eq!(args.serve_path, cwd);
        assert_eq!(args.port, default_port());
        assert_eq!(args.addrs, default_addrs());
    }

    #[test]
    fn test_args_from_config_file1() {
        let tmpdir = assert_fs::TempDir::new().unwrap();
        let config_file = tmpdir.child("config.yaml");
        let contents = format!(
            r#"
serve-path: {}
bind: 0.0.0.0
port: 3000
allow-upload: true
hidden: tmp,*.log,*.lock
"#,
            tmpdir.display()
        );
        config_file.write_str(&contents).unwrap();

        let cli = build_cli();
        let matches = cli
            .try_get_matches_from(vec!["", "-c", &config_file.to_string_lossy()])
            .unwrap();
        let args = Args::parse(matches).unwrap();
        assert_eq!(args.serve_path, Args::sanitize_path(&tmpdir).unwrap());
        assert_eq!(
            args.addrs,
            vec![BindAddr::IpAddr("0.0.0.0".parse().unwrap())]
        );
        assert_eq!(args.hidden, ["tmp", "*.log", "*.lock"]);
        assert_eq!(args.port, 3000);
        assert!(args.allow_upload);
    }

    #[test]
    fn test_args_from_config_file2() {
        let tmpdir = assert_fs::TempDir::new().unwrap();
        let config_file = tmpdir.child("config.yaml");
        let contents = r#"
bind:
  - 127.0.0.1
  - 192.168.8.10
hidden:
  - tmp
  - '*.log'
  - '*.lock'
"#;
        config_file.write_str(contents).unwrap();

        let cli = build_cli();
        let matches = cli
            .try_get_matches_from(vec!["", "-c", &config_file.to_string_lossy()])
            .unwrap();
        let args = Args::parse(matches).unwrap();
        assert_eq!(
            args.addrs,
            vec![
                BindAddr::IpAddr("127.0.0.1".parse().unwrap()),
                BindAddr::IpAddr("192.168.8.10".parse().unwrap())
            ]
        );
        assert_eq!(args.hidden, ["tmp", "*.log", "*.lock"]);
    }
}
