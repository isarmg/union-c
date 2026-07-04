use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::ServiceStatus;

#[derive(Debug, Serialize)]
pub struct RamCommandResponse {
    pub program: String,
    pub args: Vec<String>,
    pub command_line: String,
}

#[derive(Debug, Serialize)]
pub struct RamConfigResponse {
    pub serve_path: String,
    pub bind: String,
    pub port: u16,
    pub path_prefix: String,
    pub local_url: String,
    pub health_url: String,
    pub log_path: String,
    pub process_log_path: String,
    pub hidden: Vec<String>,
    pub auth_rules: Vec<String>,
    pub auth_method: String,
    pub management_auth_configured: bool,
    pub features: RamFeatures,
}

#[derive(Debug, Serialize)]
pub struct RamFeatures {
    pub allow_all: bool,
    pub allow_upload: bool,
    pub allow_delete: bool,
    pub allow_search: bool,
    pub allow_symlink: bool,
    pub allow_archive: bool,
    pub allow_hash: bool,
    pub enable_cors: bool,
    pub render_index: bool,
    pub render_try_index: bool,
    pub render_spa: bool,
    pub compress: String,
    pub assets: Option<String>,
    pub tls_enabled: bool,
}

#[derive(Debug, Serialize)]
pub struct RamHealthResponse {
    pub reachable: bool,
    pub status_code: Option<u16>,
    pub url: String,
    pub body: Option<Value>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct RamInstanceInfo {
    pub id: String,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub use_tls: bool,
    pub verify_tls: bool,
    pub reachable: bool,
    pub url: String,
    pub management_username: Option<String>,
    pub management_password_set: bool,
}

#[derive(Debug, Deserialize)]
pub struct RamInstanceSaveRequest {
    pub name: String,
    pub host: String,
    pub port: u16,
    #[serde(default)]
    pub use_tls: bool,
    #[serde(default = "default_true")]
    pub verify_tls: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Serialize)]
pub struct RamEntryResponse {
    pub url: String,
    pub path: String,
    pub status_code: u16,
    pub body: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RamAuthPath {
    pub path: String,
    pub permission: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct RamAuthRuleResponse {
    pub username: Option<String>,
    pub anonymous: bool,
    pub password_set: bool,
    pub paths: Vec<RamAuthPath>,
    pub raw: String,
}

#[derive(Debug, Serialize)]
pub struct RamAuthResponse {
    pub storage: String,
    pub auth_method: String,
    pub management_auth_configured: bool,
    pub management_username: Option<String>,
    pub rules: Vec<RamAuthRuleResponse>,
}

#[derive(Debug, Deserialize)]
pub struct RamAuthRuleInput {
    pub username: Option<String>,
    pub password: Option<String>,
    pub paths: Vec<RamAuthPath>,
}

#[derive(Debug, Deserialize)]
pub struct RamAuthUpdateRequest {
    pub rules: Vec<RamAuthRuleInput>,
    pub management_username: Option<String>,
    pub management_password: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct RamAuthUpdateResponse {
    pub saved: bool,
    pub applied: bool,
    pub ram_reloaded: bool,
    pub storage: String,
    pub management_auth_configured: bool,
    pub management_username: Option<String>,
    pub rules: Vec<RamAuthRuleResponse>,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct ActionResponse {
    pub ok: bool,
    pub message: String,
    pub service: Option<ServiceStatus>,
}
