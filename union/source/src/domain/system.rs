use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
    pub uptime_seconds: i64,
}

#[derive(Debug, Serialize)]
pub struct ReadinessResponse {
    pub status: String,
    pub database: bool,
    pub data_directory: bool,
}

#[derive(Debug, Serialize)]
pub struct DatabaseConfigResponse {
    pub configured: bool,
    pub database_url: String,
    pub connected: bool,
    pub restart_required: bool,
}

#[derive(Debug, Deserialize)]
pub struct UpdateDatabaseConfigRequest {
    pub database_url: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ServiceStatus {
    pub name: String,
    pub kind: String,
    pub runtime_state: String,
    pub healthy: bool,
    pub address: Option<String>,
    pub pid: Option<u32>,
    pub message: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize)]
pub struct LogsResponse {
    pub path: String,
    pub lines: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct SystemResources {
    pub cpu_usage_percent: f32,
    pub memory_total_kib: u64,
    pub memory_used_kib: u64,
    pub network: NetworkThroughput,
    pub disk_throughput: DiskThroughput,
    pub disks: Vec<DiskInfo>,
}

#[derive(Debug, Serialize)]
pub struct NetworkThroughput {
    pub received_bytes_per_second: u64,
    pub transmitted_bytes_per_second: u64,
    pub total_bytes_per_second: u64,
}

#[derive(Debug, Serialize)]
pub struct DiskThroughput {
    pub read_bytes_per_second: u64,
    pub write_bytes_per_second: u64,
    pub total_bytes_per_second: u64,
}

#[derive(Debug, Serialize)]
pub struct DiskInfo {
    pub name: String,
    pub mount_point: String,
    pub total_bytes: u64,
    pub available_bytes: u64,
}

#[derive(Debug, Serialize)]
pub struct EventPayload {
    pub kind: String,
    pub generated_at: String,
    pub services: Vec<ServiceStatus>,
}

#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    /// 稳定的机器可读错误码；客户端不应解析自然语言 message。
    pub code: String,
    pub error: String,
    pub message: String,
}
