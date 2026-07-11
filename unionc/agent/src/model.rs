use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const AGENT_REPORT_SCHEMA_VERSION: u16 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentReport {
    pub schema_version: u16,
    pub report_id: Uuid,
    pub collected_at: DateTime<Utc>,
    pub host: HostIdentity,
    pub interval_seconds: f64,
    pub system: SystemSnapshot,
    pub capabilities: Vec<Capability>,
    pub agent: AgentHealth,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostIdentity {
    pub id: Uuid,
    pub name: String,
    pub os: String,
    pub os_version: Option<String>,
    pub kernel_version: Option<String>,
    pub arch: String,
    pub agent_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemSnapshot {
    pub uptime_seconds: u64,
    pub cpu: CpuSnapshot,
    pub memory: MemorySnapshot,
    pub networks: Vec<NetworkSnapshot>,
    pub disks: Vec<DiskSnapshot>,
    pub temperatures: Vec<TemperatureSnapshot>,
    pub gpus: Vec<GpuSnapshot>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CpuSnapshot {
    pub usage_percent: f64,
    pub logical_count: usize,
    pub physical_count: Option<usize>,
    pub per_core_percent: Vec<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemorySnapshot {
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub available_bytes: u64,
    pub swap_total_bytes: u64,
    pub swap_used_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkSnapshot {
    pub name: String,
    pub received_bytes_total: u64,
    pub transmitted_bytes_total: u64,
    pub received_bytes_per_second: u64,
    pub transmitted_bytes_per_second: u64,
    pub packets_received_total: u64,
    pub packets_transmitted_total: u64,
    pub receive_errors_total: u64,
    pub transmit_errors_total: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiskSnapshot {
    pub name: String,
    pub mount_point: String,
    pub file_system: String,
    pub total_bytes: u64,
    pub available_bytes: u64,
    pub read_bytes_total: u64,
    pub written_bytes_total: u64,
    pub read_bytes_per_second: u64,
    pub written_bytes_per_second: u64,
    pub is_read_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TemperatureSnapshot {
    pub id: String,
    pub label: String,
    pub celsius: Option<f64>,
    pub max_celsius: Option<f64>,
    pub critical_celsius: Option<f64>,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GpuSnapshot {
    pub id: String,
    pub vendor: String,
    pub name: String,
    pub utilization_percent: Option<f64>,
    pub memory_total_bytes: Option<u64>,
    pub memory_used_bytes: Option<u64>,
    pub temperature_celsius: Option<f64>,
    pub power_watts: Option<f64>,
    pub core_clock_mhz: Option<u64>,
    pub memory_clock_mhz: Option<u64>,
    pub pcie_rx_bytes_per_second: Option<u64>,
    pub pcie_tx_bytes_per_second: Option<u64>,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Capability {
    pub name: String,
    pub available: bool,
    pub source: String,
    pub error_kind: Option<CapabilityErrorKind>,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityErrorKind {
    Unsupported,
    NotPresent,
    DriverMissing,
    PermissionDenied,
    Transient,
    InvalidData,
}

impl Capability {
    pub fn available(name: impl Into<String>, source: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            available: true,
            source: source.into(),
            error_kind: None,
            message: None,
        }
    }

    pub fn unavailable(
        name: impl Into<String>,
        source: impl Into<String>,
        error_kind: CapabilityErrorKind,
        message: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            available: false,
            source: source.into(),
            error_kind: Some(error_kind),
            message: Some(message.into()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentHealth {
    pub spool_pending_batches: u64,
    pub collector_errors: u64,
}
