use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize)]
pub struct SunshineStatus {
    pub host: String,
    pub web_port: u16,
    pub web_url: String,
    pub reachable: bool,
    pub mac_configured: bool,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct WakeResponse {
    pub ok: bool,
    pub target: String,
    pub broadcast_addr: String,
}

#[derive(Debug, Serialize)]
pub struct SunshineHostInfo {
    pub id: String,
    pub name: String,
    pub host: String,
    pub web_port: u16,
    pub mac_configured: bool,
    pub broadcast_addr: String,
    pub username: String,
    pub password_set: bool,
    pub verify_tls: bool,
    pub web_url: String,
    pub reachable: bool,
    pub connected: bool,
    pub connection_error: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SunshineHostSaveRequest {
    pub name: String,
    pub host: String,
    pub web_port: u16,
    pub mac_address: Option<String>,
    pub broadcast_addr: Option<String>,
    pub username: String,
    /// `None` 表示保留旧密码，空字符串表示清空密码。
    pub password: Option<String>,
    #[serde(default = "default_true")]
    pub verify_tls: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Deserialize)]
pub struct SunshineUnpairRequest {
    pub uuid: String,
}

#[derive(Debug, Deserialize)]
pub struct SunshineClientUpdateRequest {
    pub uuid: String,
    pub enabled: bool,
}

#[derive(Debug, Deserialize)]
pub struct SunshinePinRequest {
    pub pin: String,
    pub name: String,
}

#[derive(Debug, Deserialize)]
pub struct SunshineCoverUploadRequest {
    pub key: String,
    pub url: String,
}
