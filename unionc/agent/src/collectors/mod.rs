use std::{fs, io::Write, path::Path, time::Instant};

#[cfg(not(target_os = "linux"))]
use std::collections::HashSet;

use chrono::Utc;
use sysinfo::{Components, Disks, Networks, System};
use uuid::Uuid;

use crate::model::{
    AGENT_REPORT_SCHEMA_VERSION, AgentHealth, AgentReport, Capability, CapabilityErrorKind,
    CpuSnapshot, DiskSnapshot, GpuSnapshot, HostIdentity, MemorySnapshot, NetworkSnapshot,
    SystemSnapshot, TemperatureSnapshot,
};

#[cfg(target_os = "linux")]
mod linux_gpu;
#[cfg(target_os = "linux")]
mod linux_hwmon;
#[cfg(feature = "nvidia")]
mod nvidia;
#[cfg(target_os = "windows")]
mod windows_gpu;

/// 长期复用 sysinfo 对象，避免反复枚举系统并确保差值指标有正确采样基线。
pub struct SystemSampler {
    system: System,
    networks: Networks,
    disks: Disks,
    components: Components,
    last_sample: Instant,
    last_slow_sample: Option<Instant>,
    cached_temperatures: Vec<TemperatureSnapshot>,
    gpu_runtime: GpuRuntime,
}

impl SystemSampler {
    pub fn new() -> Self {
        Self {
            system: System::new_all(),
            networks: Networks::new_with_refreshed_list(),
            disks: Disks::new_with_refreshed_list(),
            components: {
                #[cfg(target_os = "linux")]
                {
                    Components::new()
                }
                #[cfg(not(target_os = "linux"))]
                {
                    Components::new_with_refreshed_list()
                }
            },
            last_sample: Instant::now(),
            last_slow_sample: None,
            cached_temperatures: Vec::new(),
            gpu_runtime: GpuRuntime::new(),
        }
    }

    pub fn collect(
        &mut self,
        host: HostIdentity,
        slow_interval_seconds: u64,
        spool_pending_batches: u64,
    ) -> AgentReport {
        let now = Instant::now();
        let interval_seconds = now
            .duration_since(self.last_sample)
            .as_secs_f64()
            .max(0.001);
        self.last_sample = now;

        self.system.refresh_cpu_usage();
        self.system.refresh_memory();
        self.networks.refresh(true);
        self.disks.refresh(true);

        let refresh_slow = self
            .last_slow_sample
            .is_none_or(|last| now.duration_since(last).as_secs() >= slow_interval_seconds);
        if refresh_slow {
            #[cfg(not(target_os = "linux"))]
            self.components.refresh(true);
            self.cached_temperatures = collect_temperatures(&self.components);
            self.last_slow_sample = Some(now);
        }

        let (gpus, gpu_capabilities) = self.gpu_runtime.collect();
        let mut capabilities = core_capabilities(&self.cached_temperatures);
        capabilities.extend(gpu_capabilities);
        capabilities.sort_by(|left, right| left.name.cmp(&right.name));
        capabilities.dedup_by(|left, right| left.name == right.name && left.source == right.source);
        let collector_errors = capabilities
            .iter()
            .filter(|capability| {
                !capability.available
                    && matches!(
                        capability.error_kind,
                        Some(CapabilityErrorKind::Transient | CapabilityErrorKind::InvalidData)
                    )
            })
            .count() as u64;

        AgentReport {
            schema_version: AGENT_REPORT_SCHEMA_VERSION,
            report_id: Uuid::new_v4(),
            collected_at: Utc::now(),
            host,
            interval_seconds,
            system: SystemSnapshot {
                uptime_seconds: System::uptime(),
                cpu: CpuSnapshot {
                    usage_percent: finite(self.system.global_cpu_usage() as f64).unwrap_or(0.0),
                    logical_count: self.system.cpus().len(),
                    physical_count: System::physical_core_count(),
                    per_core_percent: self
                        .system
                        .cpus()
                        .iter()
                        .map(|cpu| finite(cpu.cpu_usage() as f64).unwrap_or(0.0))
                        .collect(),
                },
                memory: MemorySnapshot {
                    total_bytes: self.system.total_memory(),
                    used_bytes: self.system.used_memory(),
                    available_bytes: self.system.available_memory(),
                    swap_total_bytes: self.system.total_swap(),
                    swap_used_bytes: self.system.used_swap(),
                },
                networks: collect_networks(&self.networks, interval_seconds),
                disks: collect_disks(&self.disks, interval_seconds),
                temperatures: self.cached_temperatures.clone(),
                gpus,
            },
            capabilities,
            agent: AgentHealth {
                spool_pending_batches,
                collector_errors,
            },
        }
    }
}

impl Default for SystemSampler {
    fn default() -> Self {
        Self::new()
    }
}

pub fn load_or_create_host_identity(
    state_dir: &Path,
    configured_id: Option<Uuid>,
) -> anyhow::Result<HostIdentity> {
    let id_path = state_dir.join("host-id");
    let id = if let Some(id) = configured_id {
        id
    } else {
        fs::create_dir_all(state_dir)?;
        set_private_directory_permissions(state_dir)?;
        if let Ok(value) = fs::read_to_string(&id_path) {
            value.trim().parse()?
        } else {
            let id = Uuid::new_v4();
            let temporary = state_dir.join(format!(".host-id-{}.tmp", Uuid::new_v4()));
            let mut options = fs::OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            let result = (|| -> anyhow::Result<()> {
                let mut file = options.open(&temporary)?;
                writeln!(file, "{id}")?;
                file.sync_all()?;
                fs::rename(&temporary, &id_path)?;
                Ok(())
            })();
            if result.is_err() {
                let _ = fs::remove_file(&temporary);
            }
            result?;
            id
        }
    };

    Ok(HostIdentity {
        id,
        name: System::host_name().unwrap_or_else(|| id.to_string()),
        os: std::env::consts::OS.to_string(),
        os_version: System::os_version(),
        kernel_version: System::kernel_version(),
        arch: std::env::consts::ARCH.to_string(),
        agent_version: env!("CARGO_PKG_VERSION").to_string(),
    })
}

#[cfg(unix)]
fn set_private_directory_permissions(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
}

#[cfg(not(unix))]
fn set_private_directory_permissions(_path: &Path) -> std::io::Result<()> {
    Ok(())
}

fn collect_networks(networks: &Networks, interval_seconds: f64) -> Vec<NetworkSnapshot> {
    networks
        .iter()
        .map(|(name, data)| NetworkSnapshot {
            name: name.clone(),
            received_bytes_total: data.total_received(),
            transmitted_bytes_total: data.total_transmitted(),
            received_bytes_per_second: per_second(data.received(), interval_seconds),
            transmitted_bytes_per_second: per_second(data.transmitted(), interval_seconds),
            packets_received_total: data.total_packets_received(),
            packets_transmitted_total: data.total_packets_transmitted(),
            receive_errors_total: data.total_errors_on_received(),
            transmit_errors_total: data.total_errors_on_transmitted(),
        })
        .collect()
}

fn collect_disks(disks: &Disks, interval_seconds: f64) -> Vec<DiskSnapshot> {
    disks
        .iter()
        .map(|disk| {
            let usage = disk.usage();
            DiskSnapshot {
                name: disk.name().to_string_lossy().into_owned(),
                mount_point: disk.mount_point().to_string_lossy().into_owned(),
                file_system: disk.file_system().to_string_lossy().into_owned(),
                total_bytes: disk.total_space(),
                available_bytes: disk.available_space(),
                read_bytes_total: usage.total_read_bytes,
                written_bytes_total: usage.total_written_bytes,
                read_bytes_per_second: per_second(usage.read_bytes, interval_seconds),
                written_bytes_per_second: per_second(usage.written_bytes, interval_seconds),
                is_read_only: disk.is_read_only(),
            }
        })
        .collect()
}

fn collect_temperatures(_components: &Components) -> Vec<TemperatureSnapshot> {
    #[cfg(not(target_os = "linux"))]
    let values: Vec<_> = _components
        .iter()
        .map(|component| TemperatureSnapshot {
            id: component
                .id()
                .map(ToOwned::to_owned)
                .unwrap_or_else(|| component.label().to_string()),
            label: component.label().to_string(),
            celsius: component
                .temperature()
                .and_then(|value| finite(value as f64)),
            // sysinfo's max() is the maximum observed by this process, not a
            // hardware threshold. Do not present it as the sensor upper limit.
            max_celsius: None,
            critical_celsius: component.critical().and_then(|value| finite(value as f64)),
            source: "sysinfo-components".to_string(),
        })
        .collect();

    #[cfg(target_os = "linux")]
    let values = linux_hwmon::collect();

    #[cfg(not(target_os = "linux"))]
    let mut seen = HashSet::new();
    #[cfg(not(target_os = "linux"))]
    let mut values = values;
    #[cfg(not(target_os = "linux"))]
    values.retain(|item| seen.insert((item.source.clone(), item.id.clone())));
    values
}

fn core_capabilities(temperatures: &[TemperatureSnapshot]) -> Vec<Capability> {
    let mut capabilities = vec![
        Capability::available("system.cpu", "sysinfo"),
        Capability::available("system.memory", "sysinfo"),
        Capability::available("system.network", "sysinfo"),
        Capability::available("system.disk", "sysinfo"),
    ];
    capabilities.push(
        if temperatures.iter().any(|value| value.celsius.is_some()) {
            Capability::available("system.temperature", "sysinfo/hwmon")
        } else {
            Capability::unavailable(
                "system.temperature",
                "sysinfo/hwmon",
                CapabilityErrorKind::Unsupported,
                "the operating system or hardware exposed no readable numeric sensor",
            )
        },
    );
    capabilities
}

struct GpuRuntime {
    #[cfg(feature = "nvidia")]
    nvidia: nvidia::NvidiaCollector,
    #[cfg(target_os = "windows")]
    windows: windows_gpu::WindowsGpuCollector,
}

impl GpuRuntime {
    fn new() -> Self {
        Self {
            #[cfg(feature = "nvidia")]
            nvidia: nvidia::NvidiaCollector::new(),
            #[cfg(target_os = "windows")]
            windows: windows_gpu::WindowsGpuCollector::new(),
        }
    }

    fn collect(&mut self) -> (Vec<GpuSnapshot>, Vec<Capability>) {
        #[allow(unused_mut)] // macOS baseline build intentionally has no private GPU collector.
        let mut gpus = Vec::new();
        let mut capabilities = Vec::new();

        #[cfg(feature = "nvidia")]
        {
            let result = self.nvidia.collect();
            gpus.extend(result.0);
            capabilities.push(result.1);
        }
        #[cfg(not(feature = "nvidia"))]
        capabilities.push(Capability::unavailable(
            "gpu.nvidia",
            "nvml",
            CapabilityErrorKind::Unsupported,
            "agent was built without the nvidia feature",
        ));

        #[cfg(target_os = "linux")]
        {
            let result = linux_gpu::collect();
            gpus.extend(result.gpus);
            capabilities.extend(result.capabilities);
        }
        #[cfg(target_os = "windows")]
        {
            let result = self.windows.collect();
            gpus.extend(result.0);
            capabilities.push(result.1);
            capabilities.push(Capability::unavailable(
                "gpu.amd.vendor",
                "amd-adlx",
                CapabilityErrorKind::Unsupported,
                "ADLX enrichment is not present; WDDM utilization remains available",
            ));
            capabilities.push(Capability::unavailable(
                "gpu.intel.vendor",
                "intel-igcl",
                CapabilityErrorKind::Unsupported,
                "IGCL enrichment is not present; WDDM utilization remains available",
            ));
        }
        #[cfg(target_os = "macos")]
        {
            capabilities.extend(platform_gpu_capabilities("metal/thermal-state"));
        }
        (gpus, capabilities)
    }
}

#[cfg(target_os = "macos")]
fn platform_gpu_capabilities(source: &str) -> Vec<Capability> {
    let platform = std::env::consts::OS;
    vec![
        Capability::unavailable(
            "gpu.amd",
            source,
            CapabilityErrorKind::Unsupported,
            format!("AMD telemetry is not enabled in the {platform} baseline build"),
        ),
        Capability::unavailable(
            "gpu.intel",
            source,
            CapabilityErrorKind::Unsupported,
            format!("Intel telemetry is not enabled in the {platform} baseline build"),
        ),
        Capability::unavailable(
            "gpu.apple",
            source,
            CapabilityErrorKind::Unsupported,
            "public APIs do not expose stable whole-system Apple GPU utilization",
        ),
    ]
}

fn per_second(delta: u64, interval_seconds: f64) -> u64 {
    (delta as f64 / interval_seconds.max(0.001)).round() as u64
}

fn finite(value: f64) -> Option<f64> {
    value.is_finite().then_some(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rate_uses_actual_interval() {
        assert_eq!(per_second(1_000, 2.0), 500);
    }

    #[test]
    fn missing_temperature_is_not_reported_as_zero() {
        let values = collect_temperatures(&Components::new());
        assert!(values.is_empty());
    }
}
