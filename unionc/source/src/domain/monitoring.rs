use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct HostIdentity {
    pub id: String,
    pub name: String,
    pub os: String,
    pub os_version: Option<String>,
    pub kernel_version: Option<String>,
    pub arch: String,
    pub agent_version: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AgentRegistrationRequest {
    #[serde(flatten)]
    pub host: HostIdentity,
    pub enrollment_secret: String,
}

impl AgentRegistrationRequest {
    pub fn validate(&self) -> AppResult<()> {
        self.host.validate()?;
        if self.enrollment_secret.len() < 32
            || self.enrollment_secret.len() > 256
            || self.enrollment_secret.chars().any(char::is_whitespace)
        {
            return Err(AppError::BadRequest(
                "enrollment_secret must contain 32 to 256 non-whitespace characters".to_string(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CapabilityReport {
    pub name: String,
    pub available: bool,
    pub source: String,
    pub error_kind: Option<String>,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AgentReport {
    pub schema_version: u16,
    pub report_id: String,
    pub collected_at: DateTime<Utc>,
    pub host: HostIdentity,
    pub interval_seconds: f64,
    pub system: SystemSnapshot,
    pub capabilities: Vec<CapabilityReport>,
    pub agent: AgentSnapshot,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SystemSnapshot {
    pub uptime_seconds: u64,
    pub cpu: CpuSnapshot,
    pub memory: MemorySnapshot,
    pub networks: Vec<NetworkSnapshot>,
    pub disks: Vec<DiskSnapshot>,
    pub temperatures: Vec<TemperatureSnapshot>,
    pub gpus: Vec<GpuSnapshot>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CpuSnapshot {
    pub usage_percent: f64,
    pub logical_count: u32,
    pub physical_count: Option<u32>,
    pub per_core_percent: Vec<f64>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MemorySnapshot {
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub available_bytes: u64,
    pub swap_total_bytes: u64,
    pub swap_used_bytes: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct NetworkSnapshot {
    pub name: String,
    pub received_bytes_total: u64,
    pub transmitted_bytes_total: u64,
    pub received_bytes_per_second: f64,
    pub transmitted_bytes_per_second: f64,
    pub packets_received_total: u64,
    pub packets_transmitted_total: u64,
    pub receive_errors_total: u64,
    pub transmit_errors_total: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DiskSnapshot {
    pub name: String,
    pub mount_point: String,
    pub file_system: String,
    pub total_bytes: u64,
    pub available_bytes: u64,
    pub read_bytes_total: u64,
    pub written_bytes_total: u64,
    pub read_bytes_per_second: f64,
    pub written_bytes_per_second: f64,
    pub is_read_only: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TemperatureSnapshot {
    pub id: String,
    pub label: String,
    pub celsius: Option<f64>,
    pub max_celsius: Option<f64>,
    pub critical_celsius: Option<f64>,
    pub source: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GpuSnapshot {
    pub id: String,
    pub vendor: String,
    pub name: String,
    pub utilization_percent: Option<f64>,
    pub memory_total_bytes: Option<u64>,
    pub memory_used_bytes: Option<u64>,
    pub temperature_celsius: Option<f64>,
    pub power_watts: Option<f64>,
    pub core_clock_mhz: Option<f64>,
    pub memory_clock_mhz: Option<f64>,
    pub pcie_rx_bytes_per_second: Option<f64>,
    pub pcie_tx_bytes_per_second: Option<f64>,
    pub source: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AgentSnapshot {
    pub spool_pending_batches: u64,
    pub collector_errors: u64,
}

#[derive(Debug, Serialize)]
pub struct AgentRegistrationResponse {
    pub host_id: String,
    pub token: String,
}

#[derive(Debug, Serialize)]
pub struct AgentReportResponse {
    pub host_id: String,
    pub report_id: String,
    pub accepted: bool,
    pub received_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct HostSummary {
    pub id: String,
    pub name: String,
    pub os: String,
    pub os_version: Option<String>,
    pub kernel_version: Option<String>,
    pub arch: String,
    pub agent_version: String,
    pub registered_at: DateTime<Utc>,
    pub last_seen_at: DateTime<Utc>,
    pub latest_collected_at: Option<DateTime<Utc>>,
    pub status: String,
    pub capabilities: Vec<CapabilityReport>,
    pub cpu_usage_percent: Option<f64>,
    pub memory_usage_percent: Option<f64>,
    pub network_received_bytes_per_second: Option<f64>,
    pub network_transmitted_bytes_per_second: Option<f64>,
    pub disk_read_bytes_per_second: Option<f64>,
    pub disk_written_bytes_per_second: Option<f64>,
    pub max_temperature_celsius: Option<f64>,
    pub gpu_utilization_percent: Option<f64>,
    pub gpu_memory_usage_percent: Option<f64>,
}

#[derive(Debug, Serialize)]
pub struct HostListResponse {
    pub hosts: Vec<HostSummary>,
}

#[derive(Debug, Serialize)]
pub struct HostDetailResponse {
    pub host: HostSummary,
    pub latest: Option<AgentReport>,
}

#[derive(Debug, Serialize)]
pub struct HistoryPoint {
    pub report_id: String,
    pub collected_at: DateTime<Utc>,
    pub received_at: DateTime<Utc>,
    pub cpu_usage_percent: Option<f64>,
    pub memory_usage_percent: Option<f64>,
    pub network_received_bytes_per_second: Option<f64>,
    pub network_transmitted_bytes_per_second: Option<f64>,
    pub disk_read_bytes_per_second: Option<f64>,
    pub disk_written_bytes_per_second: Option<f64>,
    pub max_temperature_celsius: Option<f64>,
    pub gpu_utilization_percent: Option<f64>,
    pub gpu_memory_usage_percent: Option<f64>,
}

#[derive(Debug, Serialize)]
pub struct HistoryResponse {
    pub host_id: String,
    pub points: Vec<HistoryPoint>,
}

#[derive(Debug, Deserialize)]
pub struct HistoryQuery {
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
    pub limit: Option<i64>,
}

#[derive(Debug, Clone, Default)]
pub struct MetricSummary {
    pub cpu_usage_percent: Option<f64>,
    pub memory_usage_percent: Option<f64>,
    pub network_received_bytes_per_second: Option<f64>,
    pub network_transmitted_bytes_per_second: Option<f64>,
    pub disk_read_bytes_per_second: Option<f64>,
    pub disk_written_bytes_per_second: Option<f64>,
    pub max_temperature_celsius: Option<f64>,
    pub gpu_utilization_percent: Option<f64>,
    pub gpu_memory_usage_percent: Option<f64>,
}

impl HostIdentity {
    pub fn validate(&self) -> AppResult<()> {
        uuid::Uuid::parse_str(&self.id)
            .map_err(|_| AppError::BadRequest("host.id must be a UUID".to_string()))?;
        validate_text("host.name", &self.name, 255)?;
        validate_text("host.os", &self.os, 64)?;
        validate_text("host.arch", &self.arch, 64)?;
        validate_text("host.agent_version", &self.agent_version, 128)
    }
}

impl AgentReport {
    pub fn validate(&self) -> AppResult<()> {
        self.host.validate()?;
        uuid::Uuid::parse_str(&self.report_id)
            .map_err(|_| AppError::BadRequest("report_id must be a UUID".to_string()))?;
        if self.schema_version != 1 {
            return Err(AppError::BadRequest(
                "unsupported agent report schema_version".to_string(),
            ));
        }
        if !self.interval_seconds.is_finite() || !(0.1..=3600.0).contains(&self.interval_seconds) {
            return Err(AppError::BadRequest(
                "interval_seconds is outside the supported range".to_string(),
            ));
        }
        if self.collected_at > Utc::now() + chrono::Duration::minutes(5) {
            return Err(AppError::BadRequest(
                "collected_at is too far in the future".to_string(),
            ));
        }
        if self.capabilities.len() > 256
            || self.system.networks.len() > 1024
            || self.system.disks.len() > 1024
            || self.system.temperatures.len() > 4096
            || self.system.gpus.len() > 128
        {
            return Err(AppError::BadRequest(
                "report contains too many devices".to_string(),
            ));
        }
        validate_percent("cpu.usage_percent", self.system.cpu.usage_percent)?;
        if self.system.cpu.logical_count == 0 {
            return Err(AppError::BadRequest(
                "cpu.logical_count must be positive".to_string(),
            ));
        }
        for value in &self.system.cpu.per_core_percent {
            validate_percent("cpu.per_core_percent", *value)?;
        }
        if self.system.memory.used_bytes > self.system.memory.total_bytes
            || self.system.memory.available_bytes > self.system.memory.total_bytes
            || self.system.memory.swap_used_bytes > self.system.memory.swap_total_bytes
        {
            return Err(AppError::BadRequest(
                "memory counters exceed their reported totals".to_string(),
            ));
        }
        for network in &self.system.networks {
            validate_text("network.name", &network.name, 255)?;
            validate_nonnegative_rate(
                "network.received_bytes_per_second",
                network.received_bytes_per_second,
            )?;
            validate_nonnegative_rate(
                "network.transmitted_bytes_per_second",
                network.transmitted_bytes_per_second,
            )?;
        }
        for disk in &self.system.disks {
            validate_text("disk.name", &disk.name, 1024)?;
            validate_text("disk.mount_point", &disk.mount_point, 4096)?;
            if disk.available_bytes > disk.total_bytes {
                return Err(AppError::BadRequest(
                    "disk available bytes exceed total bytes".to_string(),
                ));
            }
            validate_nonnegative_rate("disk.read_bytes_per_second", disk.read_bytes_per_second)?;
            validate_nonnegative_rate(
                "disk.written_bytes_per_second",
                disk.written_bytes_per_second,
            )?;
        }
        for sensor in &self.system.temperatures {
            for (field, value) in [
                ("temperature.celsius", sensor.celsius),
                ("temperature.max_celsius", sensor.max_celsius),
                ("temperature.critical_celsius", sensor.critical_celsius),
            ] {
                if value
                    .is_some_and(|value| !value.is_finite() || !(-273.15..=1000.0).contains(&value))
                {
                    return Err(AppError::BadRequest(format!("invalid {field}")));
                }
            }
        }
        for gpu in &self.system.gpus {
            if let Some(value) = gpu.utilization_percent {
                validate_percent("gpu.utilization_percent", value)?;
            }
            if gpu
                .memory_used_bytes
                .zip(gpu.memory_total_bytes)
                .is_some_and(|(used, total)| used > total)
            {
                return Err(AppError::BadRequest(
                    "GPU memory usage exceeds total memory".to_string(),
                ));
            }
            if gpu
                .temperature_celsius
                .is_some_and(|value| !value.is_finite() || !(-273.15..=1000.0).contains(&value))
            {
                return Err(AppError::BadRequest(
                    "invalid gpu.temperature_celsius".to_string(),
                ));
            }
            for (field, value) in [
                ("gpu.power_watts", gpu.power_watts),
                ("gpu.core_clock_mhz", gpu.core_clock_mhz),
                ("gpu.memory_clock_mhz", gpu.memory_clock_mhz),
                ("gpu.pcie_rx_bytes_per_second", gpu.pcie_rx_bytes_per_second),
                ("gpu.pcie_tx_bytes_per_second", gpu.pcie_tx_bytes_per_second),
            ] {
                if let Some(value) = value {
                    validate_nonnegative_rate(field, value)?;
                }
            }
        }
        Ok(())
    }

    pub fn metric_summary(&self) -> MetricSummary {
        let memory_usage_percent = (self.system.memory.total_bytes > 0).then(|| {
            self.system.memory.used_bytes as f64 * 100.0 / self.system.memory.total_bytes as f64
        });
        let max_sensor_temperature = self
            .system
            .temperatures
            .iter()
            .filter_map(|sensor| sensor.celsius)
            .chain(
                self.system
                    .gpus
                    .iter()
                    .filter_map(|gpu| gpu.temperature_celsius),
            )
            .reduce(f64::max);
        let gpu_memory = self
            .system
            .gpus
            .iter()
            .filter_map(|gpu| gpu.memory_used_bytes.zip(gpu.memory_total_bytes))
            .fold((0_u64, 0_u64), |sum, (used, total)| {
                (sum.0.saturating_add(used), sum.1.saturating_add(total))
            });
        MetricSummary {
            cpu_usage_percent: Some(self.system.cpu.usage_percent),
            memory_usage_percent,
            network_received_bytes_per_second: self
                .system
                .networks
                .iter()
                .map(|item| item.received_bytes_per_second)
                .reduce(f64::max),
            network_transmitted_bytes_per_second: self
                .system
                .networks
                .iter()
                .map(|item| item.transmitted_bytes_per_second)
                .reduce(f64::max),
            disk_read_bytes_per_second: self
                .system
                .disks
                .iter()
                .map(|item| item.read_bytes_per_second)
                .reduce(f64::max),
            disk_written_bytes_per_second: self
                .system
                .disks
                .iter()
                .map(|item| item.written_bytes_per_second)
                .reduce(f64::max),
            max_temperature_celsius: max_sensor_temperature,
            gpu_utilization_percent: self
                .system
                .gpus
                .iter()
                .filter_map(|gpu| gpu.utilization_percent)
                .reduce(f64::max),
            gpu_memory_usage_percent: (gpu_memory.1 > 0)
                .then(|| gpu_memory.0 as f64 * 100.0 / gpu_memory.1 as f64),
        }
    }
}

fn validate_text(field: &str, value: &str, max: usize) -> AppResult<()> {
    if value.trim().is_empty() || value.len() > max || value.chars().any(char::is_control) {
        return Err(AppError::BadRequest(format!("invalid {field}")));
    }
    Ok(())
}

fn validate_percent(field: &str, value: f64) -> AppResult<()> {
    if !value.is_finite() || !(0.0..=100.0).contains(&value) {
        return Err(AppError::BadRequest(format!("invalid {field}")));
    }
    Ok(())
}

fn validate_nonnegative_rate(field: &str, value: f64) -> AppResult<()> {
    if !value.is_finite() || value < 0.0 {
        return Err(AppError::BadRequest(format!("invalid {field}")));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summary_uses_largest_device_rate_instead_of_double_counting() {
        let report: AgentReport = serde_json::from_value(serde_json::json!({
            "schema_version": 1,
            "report_id": "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
            "collected_at": "2026-01-01T00:00:00Z",
            "host": {
                "id": "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb",
                "name": "host", "os": "linux", "os_version": null,
                "kernel_version": null, "arch": "x86_64", "agent_version": "0.1.0"
            },
            "interval_seconds": 10.0,
            "system": {
                "uptime_seconds": 1,
                "cpu": {"usage_percent": 5.0, "logical_count": 1, "physical_count": null, "per_core_percent": [5.0]},
                "memory": {"total_bytes": 100, "used_bytes": 50, "available_bytes": 50, "swap_total_bytes": 0, "swap_used_bytes": 0},
                "networks": [
                    {"name":"eth0","received_bytes_total":1,"transmitted_bytes_total":1,"received_bytes_per_second":100.0,"transmitted_bytes_per_second":40.0,"packets_received_total":1,"packets_transmitted_total":1,"receive_errors_total":0,"transmit_errors_total":0},
                    {"name":"bridge0","received_bytes_total":1,"transmitted_bytes_total":1,"received_bytes_per_second":80.0,"transmitted_bytes_per_second":70.0,"packets_received_total":1,"packets_transmitted_total":1,"receive_errors_total":0,"transmit_errors_total":0}
                ],
                "disks": [
                    {"name":"sda","mount_point":"/","file_system":"ext4","total_bytes":1,"available_bytes":1,"read_bytes_total":1,"written_bytes_total":1,"read_bytes_per_second":30.0,"written_bytes_per_second":60.0,"is_read_only":false},
                    {"name":"bind","mount_point":"/bind","file_system":"ext4","total_bytes":1,"available_bytes":1,"read_bytes_total":1,"written_bytes_total":1,"read_bytes_per_second":20.0,"written_bytes_per_second":50.0,"is_read_only":false}
                ],
                "temperatures": [], "gpus": []
            },
            "capabilities": [],
            "agent": {"spool_pending_batches": 0, "collector_errors": 0}
        }))
        .expect("valid report");

        let summary = report.metric_summary();
        assert_eq!(summary.network_received_bytes_per_second, Some(100.0));
        assert_eq!(summary.network_transmitted_bytes_per_second, Some(70.0));
        assert_eq!(summary.disk_read_bytes_per_second, Some(30.0));
        assert_eq!(summary.disk_written_bytes_per_second, Some(60.0));
    }
}
