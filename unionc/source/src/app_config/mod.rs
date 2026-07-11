//! UnionC 控制台配置模型。

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const LOCAL_CONFIG_PATH: &str = "unionc/data/unionc-config.json";

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LocalConfig {
    #[serde(default)]
    pub database_url: String,
    pub admin_username: String,
    pub admin_password_hash: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum LocalConfigError {
    #[error("数据库连接地址不能为空")]
    EmptyDatabaseUrl,
    #[error("数据库连接地址格式无效")]
    InvalidDatabaseUrl,
    #[error("仅支持 PostgreSQL 连接地址")]
    UnsupportedDatabaseScheme,
    #[error("local admin username cannot be empty")]
    EmptyAdminUsername,
    #[error("local admin username contains invalid characters")]
    InvalidAdminUsername,
    #[error("local admin password hash cannot be empty")]
    EmptyAdminPasswordHash,
    #[error("local admin password hash must be a valid bcrypt hash")]
    InvalidAdminPasswordHash,
}

impl LocalConfigError {
    pub fn code(self) -> &'static str {
        match self {
            Self::EmptyDatabaseUrl => "local_config_database_url_empty",
            Self::InvalidDatabaseUrl => "local_config_database_url_invalid",
            Self::UnsupportedDatabaseScheme => "local_config_database_url_unsupported_scheme",
            Self::EmptyAdminUsername => "local_config_admin_username_empty",
            Self::InvalidAdminUsername => "local_config_admin_username_invalid",
            Self::EmptyAdminPasswordHash => "local_config_admin_password_hash_empty",
            Self::InvalidAdminPasswordHash => "local_config_admin_password_hash_invalid",
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct Settings {
    #[serde(skip)]
    pub production: bool,
    pub server: ServerSettings,
    #[serde(skip)]
    pub database: DatabaseSettings,
    pub paths: PathSettings,
    pub sunshine: SunshineSettings,
    #[serde(skip)]
    pub agents: AgentSettings,
}

#[derive(Debug, Clone, Default)]
pub struct AgentSettings {
    pub enrollment_token: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct ServerSettings {
    pub bind: String,
    pub port: u16,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct DatabaseSettings {
    pub url: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct PathSettings {
    pub data_dir: PathBuf,
    pub sunshine_dir: PathBuf,
    pub moonlight_dir: PathBuf,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct SunshineHostConfig {
    pub id: String,
    pub name: String,
    pub host: String,
    pub web_port: u16,
    pub mac_address: Option<String>,
    pub broadcast_addr: String,
    pub log_path: PathBuf,
    pub username: String,
    pub password: String,
    pub verify_tls: bool,
}

impl Default for SunshineHostConfig {
    fn default() -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: "Sunshine".to_string(),
            host: "127.0.0.1".to_string(),
            web_port: 47990,
            mac_address: None,
            broadcast_addr: "255.255.255.255:9".to_string(),
            log_path: PathBuf::from("unionc/data/sunshine/logs/sunshine.log"),
            username: "admin".to_string(),
            password: String::new(),
            verify_tls: true,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct SunshineSettings {
    pub hosts: Vec<SunshineHostConfig>,
}

impl Default for ServerSettings {
    fn default() -> Self {
        Self {
            bind: "127.0.0.1".to_string(),
            port: 8081,
        }
    }
}

impl Default for PathSettings {
    fn default() -> Self {
        Self {
            data_dir: PathBuf::from("unionc/data"),
            sunshine_dir: PathBuf::from("unionc/data/sunshine"),
            moonlight_dir: PathBuf::from("unionc/data/moonlight"),
        }
    }
}

impl Default for SunshineSettings {
    fn default() -> Self {
        Self {
            hosts: vec![SunshineHostConfig::default()],
        }
    }
}

mod runtime;

pub use runtime::{ensure_layout, load_local_config, normalize_database_url, save_local_config};
