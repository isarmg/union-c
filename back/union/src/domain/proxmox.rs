use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize)]
pub struct PveHostInfo {
    pub id: String,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub token_id: String,
    pub token_secret_set: bool,
    pub verify_tls: bool,
    pub web_url: String,
    pub connected: bool,
    pub connection_error: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct PveHostSaveRequest {
    pub name: String,
    pub host: String,
    pub port: u16,
    pub token_id: String,
    /// `None` 表示保留旧 token secret。
    pub token_secret: Option<String>,
    pub verify_tls: bool,
}

#[derive(Debug, Deserialize)]
pub struct PveSnapshotRequest {
    pub snapname: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub vmstate: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct PveMigrateRequest {
    pub target: String,
    #[serde(default)]
    pub online: Option<bool>,
    #[serde(default)]
    pub with_local_disks: Option<bool>,
}

#[derive(Debug, Deserialize, Default)]
pub struct PveDeleteQuery {
    #[serde(default)]
    pub purge: Option<bool>,
    #[serde(rename = "destroy_unreferenced_disks", default)]
    pub destroy_unreferenced_disks: Option<bool>,
}
