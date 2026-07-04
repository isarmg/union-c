//! 管理中心配置加载与目录初始化。
//!
//! 初学者可以把这个文件看成“程序启动前的说明书读取器”：
//! - `Settings::default()` 提供一套能本地运行的默认值；
//! - `Settings::load()` 只读取数据库启动连接串，运行配置保存在 PostgreSQL；
//! - ram 敏感配置由服务管理层写入私有 YAML 文件；
//! - `ensure_layout()` 确保数据、资源和日志目录都存在。

use std::{
    env, fs,
    net::IpAddr,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

pub const LOCAL_CONFIG_PATH: &str = "data/union-config.json";

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LocalConfig {
    #[serde(default)]
    pub database_url: String,
    pub admin_username: String,
    pub admin_password_hash: String,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
/// 整个管理中心的总配置。
///
/// `#[serde(default)]` 的意思是：反序列化配置文件时，如果某个字段没写，
/// 就使用对应 `Default` 实现里的默认值，而不是直接报错。
pub struct Settings {
    /// 仅由启动环境决定，不持久化到数据库。
    #[serde(skip)]
    pub production: bool,
    /// 管理后端 HTTP 服务监听配置。
    pub server: ServerSettings,
    /// PostgreSQL 数据库连接配置。
    #[serde(skip)]
    pub database: DatabaseSettings,
    /// 项目中各类数据目录路径。
    pub paths: PathSettings,
    /// ram 文件服务启动参数。
    pub ram: RamSettings,
    /// Proxmox VE 多主机管理配置。
    pub proxmox: ProxmoxSettings,
    /// Sunshine/Moonlight 相关探测和唤醒配置。
    pub sunshine: SunshineSettings,
    /// Astro 博客构建配置。
    pub blog: BlogSettings,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
/// 管理中心自身的 HTTP 监听地址。
pub struct ServerSettings {
    /// 监听 IP，例如 127.0.0.1 表示只允许本机访问。
    pub bind: String,
    /// 监听端口，例如 8080。
    pub port: u16,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
/// 数据库连接配置。
pub struct DatabaseSettings {
    /// PostgreSQL 连接字符串，格式通常是 postgresql://user:password@host:port/database。
    pub url: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
/// 项目中的数据目录。
///
/// 这里集中管理目录路径，是为了避免业务代码到处硬编码字符串。
pub struct PathSettings {
    /// ram 对外提供文件访问的根目录。
    pub data_dir: PathBuf,
    /// 公开文件目录。
    pub public_dir: PathBuf,
    /// 收件箱/上传暂存目录。
    pub inbox_dir: PathBuf,
    /// 私有文件目录。
    pub private_dir: PathBuf,
    /// 从 PostgreSQL 导出的博客构建目录。Astro 前台只读取这里。
    pub blog_export_dir: PathBuf,
    /// 博客图片、附件等静态资源目录，由博客前台直接消费，不经过 ram。
    pub blog_assets_dir: PathBuf,
    /// 媒体资源目录。
    pub media_dir: PathBuf,
    /// Sunshine 独立目录。
    pub sunshine_dir: PathBuf,
    /// Moonlight 独立目录。
    pub moonlight_dir: PathBuf,
    /// 日志目录。
    pub logs_dir: PathBuf,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
/// ram 文件服务的启动配置。
///
/// 大部分字段会被 `ram_args_with_auth()` 转换成命令行参数。
pub struct RamSettings {
    /// 浏览器访问 ram 的公开 HTTPS 地址，仅由环境变量注入，不写入数据库。
    #[serde(skip)]
    pub public_url: Option<String>,
    /// ram 可执行文件名或绝对路径。默认 "ram" 从 PATH 查找。
    pub command: String,
    /// ram 监听 IP。
    pub bind: String,
    /// ram 监听端口。
    pub port: u16,
    /// 反向代理路径前缀，例如 /files。
    pub path_prefix: String,
    /// 是否允许所有操作；开启后权限非常大，生产环境要谨慎。
    pub allow_all: bool,
    /// 是否允许上传。
    pub allow_upload: bool,
    /// 是否允许删除。
    pub allow_delete: bool,
    /// 是否允许搜索。
    pub allow_search: bool,
    /// 是否允许跟随符号链接。
    pub allow_symlink: bool,
    /// 是否允许打包下载。
    pub allow_archive: bool,
    /// 是否允许计算文件 hash。
    pub allow_hash: bool,
    /// 是否开启 CORS。
    pub enable_cors: bool,
    /// 是否渲染目录首页。
    pub render_index: bool,
    /// 是否尝试渲染 index.html。
    pub render_try_index: bool,
    /// 是否按 SPA 模式回退。
    pub render_spa: bool,
    /// 隐藏文件规则，例如 .git、*.tmp。
    pub hidden: Vec<String>,
    /// 原始 ram 认证规则。首次启动或数据库未初始化时会作为种子数据。
    pub auth: Vec<String>,
    /// 认证方式，ram 支持 basic/digest。
    pub auth_method: String,
    /// 管理接口访问 ram 时使用的账号密码，格式 username:password。
    pub management_auth: Option<String>,
    /// 自定义 ram 前端静态资源目录。
    pub assets: Option<PathBuf>,
    /// ram HTTP 日志格式。
    pub log_format: Option<String>,
    /// 压缩等级，例如 none/low/medium/high。
    pub compress: String,
    /// TLS 证书路径；和 tls_key 同时配置时启用 HTTPS。
    pub tls_cert: Option<PathBuf>,
    /// TLS 私钥路径。
    pub tls_key: Option<PathBuf>,
    /// 额外透传给 ram 的命令行参数。
    pub extra_args: Vec<String>,
    /// ram 自身访问日志路径。
    pub log_path: PathBuf,
    /// 管理中心捕获 ram 标准输出/错误输出的日志路径。
    pub process_log_path: PathBuf,
}

/// 单台 Sunshine 主机的完整配置。
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct SunshineHostConfig {
    /// 主机唯一标识，由 UUID 自动生成。
    pub id: String,
    /// 显示名称，例如 "游戏主机"。
    pub name: String,
    /// Sunshine Web UI 所在 IP 或主机名。
    pub host: String,
    /// Sunshine Web UI 端口（默认 47990）。
    pub web_port: u16,
    /// WOL 所需目标网卡 MAC 地址。
    pub mac_address: Option<String>,
    /// WOL 广播地址，默认 255.255.255.255:9。
    pub broadcast_addr: String,
    /// Sunshine 本地日志路径（用于读取日志文件）。
    pub log_path: PathBuf,
    /// Sunshine Web UI 管理用户名。
    pub username: String,
    /// Sunshine Web UI 管理密码。
    pub password: String,
    /// 是否验证 Sunshine HTTPS 证书；默认开启，可为自签名证书显式关闭。
    pub verify_tls: bool,
}

impl Default for SunshineHostConfig {
    fn default() -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: "Sunshine".to_string(),
            host: "172.29.160.1".to_string(),
            web_port: 47990,
            mac_address: None,
            broadcast_addr: "255.255.255.255:9".to_string(),
            log_path: PathBuf::from("data/sunshine/logs/sunshine.log"),
            username: "admin".to_string(),
            password: String::new(),
            verify_tls: true,
        }
    }
}

fn default_sunshine_hosts() -> Vec<SunshineHostConfig> {
    vec![SunshineHostConfig::default()]
}

/// 单台 Proxmox VE 主机配置。
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct ProxmoxHostConfig {
    /// 主机唯一标识，UUID 自动生成。
    pub id: String,
    /// 显示名称，例如 "家庭 PVE"。
    pub name: String,
    /// PVE 主机 IP 或域名。
    pub host: String,
    /// PVE Web UI 端口，默认 8006。
    pub port: u16,
    /// API Token ID，格式 user@realm!tokenname，例如 root@pam!mytoken。
    pub token_id: String,
    /// API Token 密钥（UUID 格式）。
    pub token_secret: String,
    /// 是否验证 TLS 证书；自签名环境应安装内部 CA，而不是关闭验证。
    pub verify_tls: bool,
}

impl Default for ProxmoxHostConfig {
    fn default() -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: "Proxmox VE".to_string(),
            host: "192.168.1.1".to_string(),
            port: 8006,
            token_id: "root@pam!mytoken".to_string(),
            token_secret: String::new(),
            verify_tls: true,
        }
    }
}

/// Proxmox VE 多主机管理配置。
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct ProxmoxSettings {
    pub hosts: Vec<ProxmoxHostConfig>,
}

/// Sunshine/Moonlight 多主机管理配置。
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct SunshineSettings {
    /// 所有被管理的 Sunshine 主机列表。
    #[serde(default = "default_sunshine_hosts")]
    pub hosts: Vec<SunshineHostConfig>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
/// Astro 博客构建配置。
pub struct BlogSettings {
    /// 博客项目目录。
    pub work_dir: PathBuf,
    /// 构建命令，例如 npm。
    pub build_command: String,
    /// 构建命令参数，例如 ["run", "build"]。
    pub build_args: Vec<String>,
    /// 博客构建日志目录。
    pub build_log_dir: PathBuf,
}

impl Default for ServerSettings {
    fn default() -> Self {
        Self {
            bind: "127.0.0.1".to_string(),
            port: 8080,
        }
    }
}

impl Default for PathSettings {
    fn default() -> Self {
        Self {
            data_dir: PathBuf::from("data/ram/files"),
            public_dir: PathBuf::from("data/ram/files/public"),
            inbox_dir: PathBuf::from("data/ram/files/inbox"),
            private_dir: PathBuf::from("data/ram/files/private"),
            blog_export_dir: PathBuf::from("data/blog/content"),
            blog_assets_dir: PathBuf::from("data/blog/files"),
            media_dir: PathBuf::from("data/ram/files/media"),
            sunshine_dir: PathBuf::from("data/sunshine"),
            moonlight_dir: PathBuf::from("data/moonlight"),
            logs_dir: PathBuf::from("data/ram/logs"),
        }
    }
}

impl Default for RamSettings {
    fn default() -> Self {
        Self {
            command: "ram".to_string(),
            public_url: None,
            bind: "127.0.0.1".to_string(),
            port: 5000,
            path_prefix: "/files".to_string(),
            allow_all: false,
            allow_upload: false,
            allow_delete: false,
            allow_search: true,
            allow_symlink: false,
            allow_archive: true,
            allow_hash: true,
            enable_cors: false,
            render_index: false,
            render_try_index: false,
            render_spa: false,
            hidden: vec![
                ".git".to_string(),
                "*.tmp".to_string(),
                "*.lock".to_string(),
            ],
            auth: Vec::new(),
            auth_method: "digest".to_string(),
            management_auth: None,
            assets: None,
            log_format: Some(
                r#"$time_iso8601 $log_level - $remote_addr "$request" $status"#.to_string(),
            ),
            compress: "low".to_string(),
            tls_cert: None,
            tls_key: None,
            extra_args: Vec::new(),
            log_path: PathBuf::from("data/ram/logs/ram.log"),
            process_log_path: PathBuf::from("data/ram/logs/ram-process.log"),
        }
    }
}

impl Default for SunshineSettings {
    fn default() -> Self {
        Self {
            hosts: default_sunshine_hosts(),
        }
    }
}

impl Default for BlogSettings {
    fn default() -> Self {
        Self {
            work_dir: PathBuf::from("back/blog"),
            build_command: "npm".to_string(),
            build_args: vec!["run".to_string(), "build".to_string()],
            build_log_dir: PathBuf::from("data/blog/logs"),
        }
    }
}

mod runtime;

pub use runtime::{ensure_layout, load_local_config, save_local_config};
