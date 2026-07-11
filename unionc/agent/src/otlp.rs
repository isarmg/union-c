//! 仅包含 Agent 所需的 OTLP Metrics Protobuf 子集。
//!
//! 字段编号严格对应 OpenTelemetry proto；保留自有 JSON report 作为资产/capability
//! 数据源，OTLP 只承载时序数值。

use prost::{Message, Oneof};

use crate::model::{AgentReport, GpuSnapshot};

#[derive(Clone, PartialEq, Message)]
pub struct ExportMetricsServiceRequest {
    #[prost(message, repeated, tag = "1")]
    pub resource_metrics: Vec<ResourceMetrics>,
}

#[derive(Clone, PartialEq, Message)]
pub struct ResourceMetrics {
    #[prost(message, optional, tag = "1")]
    pub resource: Option<Resource>,
    #[prost(message, repeated, tag = "2")]
    pub scope_metrics: Vec<ScopeMetrics>,
    #[prost(string, tag = "3")]
    pub schema_url: String,
}

#[derive(Clone, PartialEq, Message)]
pub struct Resource {
    #[prost(message, repeated, tag = "1")]
    pub attributes: Vec<KeyValue>,
    #[prost(uint32, tag = "2")]
    pub dropped_attributes_count: u32,
}

#[derive(Clone, PartialEq, Message)]
pub struct KeyValue {
    #[prost(string, tag = "1")]
    pub key: String,
    #[prost(message, optional, tag = "2")]
    pub value: Option<AnyValue>,
}

#[derive(Clone, PartialEq, Message)]
pub struct AnyValue {
    #[prost(oneof = "any_value::Value", tags = "1, 2, 3, 4")]
    pub value: Option<any_value::Value>,
}

pub mod any_value {
    use super::*;

    #[derive(Clone, PartialEq, Oneof)]
    pub enum Value {
        #[prost(string, tag = "1")]
        StringValue(String),
        #[prost(bool, tag = "2")]
        BoolValue(bool),
        #[prost(int64, tag = "3")]
        IntValue(i64),
        #[prost(double, tag = "4")]
        DoubleValue(f64),
    }
}

#[derive(Clone, PartialEq, Message)]
pub struct ScopeMetrics {
    #[prost(message, optional, tag = "1")]
    pub scope: Option<InstrumentationScope>,
    #[prost(message, repeated, tag = "2")]
    pub metrics: Vec<Metric>,
    #[prost(string, tag = "3")]
    pub schema_url: String,
}

#[derive(Clone, PartialEq, Message)]
pub struct InstrumentationScope {
    #[prost(string, tag = "1")]
    pub name: String,
    #[prost(string, tag = "2")]
    pub version: String,
    #[prost(message, repeated, tag = "3")]
    pub attributes: Vec<KeyValue>,
    #[prost(uint32, tag = "4")]
    pub dropped_attributes_count: u32,
}

#[derive(Clone, PartialEq, Message)]
pub struct Metric {
    #[prost(string, tag = "1")]
    pub name: String,
    #[prost(string, tag = "2")]
    pub description: String,
    #[prost(string, tag = "3")]
    pub unit: String,
    #[prost(oneof = "metric::Data", tags = "5, 7")]
    pub data: Option<metric::Data>,
}

pub mod metric {
    use super::*;

    #[derive(Clone, PartialEq, Oneof)]
    pub enum Data {
        #[prost(message, tag = "5")]
        Gauge(Gauge),
        #[prost(message, tag = "7")]
        Sum(Sum),
    }
}

#[derive(Clone, PartialEq, Message)]
pub struct Gauge {
    #[prost(message, repeated, tag = "1")]
    pub data_points: Vec<NumberDataPoint>,
}

#[derive(Clone, PartialEq, Message)]
pub struct Sum {
    #[prost(message, repeated, tag = "1")]
    pub data_points: Vec<NumberDataPoint>,
    #[prost(enumeration = "AggregationTemporality", tag = "2")]
    pub aggregation_temporality: i32,
    #[prost(bool, tag = "3")]
    pub is_monotonic: bool,
}

#[derive(Clone, PartialEq, Message)]
pub struct NumberDataPoint {
    #[prost(message, repeated, tag = "7")]
    pub attributes: Vec<KeyValue>,
    #[prost(fixed64, tag = "2")]
    pub start_time_unix_nano: u64,
    #[prost(fixed64, tag = "3")]
    pub time_unix_nano: u64,
    #[prost(oneof = "number_data_point::Value", tags = "4, 6")]
    pub value: Option<number_data_point::Value>,
    #[prost(uint32, tag = "8")]
    pub flags: u32,
}

pub mod number_data_point {
    use super::*;

    #[derive(Clone, PartialEq, Oneof)]
    pub enum Value {
        #[prost(double, tag = "4")]
        AsDouble(f64),
        #[prost(sfixed64, tag = "6")]
        AsInt(i64),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, prost::Enumeration)]
#[repr(i32)]
pub enum AggregationTemporality {
    Unspecified = 0,
    Delta = 1,
    Cumulative = 2,
}

pub fn encode_report(report: &AgentReport) -> ExportMetricsServiceRequest {
    let time = report
        .collected_at
        .timestamp_nanos_opt()
        .unwrap_or_default()
        .max(0) as u64;
    let start_time =
        time.saturating_sub(report.system.uptime_seconds.saturating_mul(1_000_000_000));
    let mut metrics = vec![
        gauge(
            "system.cpu.utilization",
            "1",
            report.system.cpu.usage_percent / 100.0,
            time,
            vec![],
        ),
        gauge(
            "system.memory.usage",
            "By",
            report.system.memory.used_bytes as f64,
            time,
            vec![attr("system.memory.state", "used")],
        ),
        gauge(
            "system.memory.limit",
            "By",
            report.system.memory.total_bytes as f64,
            time,
            vec![],
        ),
        gauge(
            "system.uptime",
            "s",
            report.system.uptime_seconds as f64,
            time,
            vec![],
        ),
    ];
    for network in &report.system.networks {
        let attributes = vec![attr("network.interface.name", &network.name)];
        metrics.push(sum(
            "system.network.io",
            "By",
            network.received_bytes_total,
            start_time,
            time,
            with_attr(&attributes, "network.io.direction", "receive"),
        ));
        metrics.push(sum(
            "system.network.io",
            "By",
            network.transmitted_bytes_total,
            start_time,
            time,
            with_attr(&attributes, "network.io.direction", "transmit"),
        ));
    }
    for disk in &report.system.disks {
        let attributes = vec![
            attr("system.device", &disk.name),
            attr("system.filesystem.mountpoint", &disk.mount_point),
        ];
        metrics.push(gauge(
            "system.filesystem.usage",
            "By",
            (disk.total_bytes.saturating_sub(disk.available_bytes)) as f64,
            time,
            with_attr(&attributes, "system.filesystem.state", "used"),
        ));
        metrics.push(sum(
            "system.disk.io",
            "By",
            disk.read_bytes_total,
            start_time,
            time,
            with_attr(&attributes, "disk.io.direction", "read"),
        ));
        metrics.push(sum(
            "system.disk.io",
            "By",
            disk.written_bytes_total,
            start_time,
            time,
            with_attr(&attributes, "disk.io.direction", "write"),
        ));
    }
    for temperature in &report.system.temperatures {
        if let Some(value) = temperature.celsius {
            metrics.push(gauge(
                "hw.temperature",
                "Cel",
                value,
                time,
                vec![
                    attr("sensor.id", &temperature.id),
                    attr("sensor.label", &temperature.label),
                    attr("telemetry.source", &temperature.source),
                ],
            ));
        }
    }
    for gpu in &report.system.gpus {
        append_gpu_metrics(&mut metrics, gpu, time);
    }

    ExportMetricsServiceRequest {
        resource_metrics: vec![ResourceMetrics {
            resource: Some(Resource {
                attributes: vec![
                    attr("host.id", &report.host.id.to_string()),
                    attr("host.name", &report.host.name),
                    attr("os.type", otel_os_type(&report.host.os)),
                    attr("host.arch", otel_host_arch(&report.host.arch)),
                    attr("service.name", "unionc-agent"),
                    attr("service.version", &report.host.agent_version),
                ],
                dropped_attributes_count: 0,
            }),
            scope_metrics: vec![ScopeMetrics {
                scope: Some(InstrumentationScope {
                    name: "unionc.agent.hostmetrics".into(),
                    version: report.host.agent_version.clone(),
                    attributes: Vec::new(),
                    dropped_attributes_count: 0,
                }),
                metrics,
                schema_url: "https://unionc.local/schemas/hostmetrics/1".into(),
            }],
            schema_url: "https://opentelemetry.io/schemas/1.36.0".into(),
        }],
    }
}

fn otel_os_type(value: &str) -> &str {
    match value {
        "macos" => "darwin",
        other => other,
    }
}

fn otel_host_arch(value: &str) -> &str {
    match value {
        "x86_64" => "amd64",
        "aarch64" => "arm64",
        "x86" | "i386" | "i586" | "i686" => "x86",
        other => other,
    }
}

fn append_gpu_metrics(metrics: &mut Vec<Metric>, gpu: &GpuSnapshot, time: u64) {
    let attrs = vec![
        attr("gpu.id", &gpu.id),
        attr("gpu.vendor", &gpu.vendor),
        attr("gpu.name", &gpu.name),
    ];
    if let Some(value) = gpu.utilization_percent {
        metrics.push(gauge(
            "hw.gpu.utilization",
            "1",
            value / 100.0,
            time,
            attrs.clone(),
        ));
    }
    if let Some(value) = gpu.memory_used_bytes {
        metrics.push(gauge(
            "hw.gpu.memory.usage",
            "By",
            value as f64,
            time,
            attrs.clone(),
        ));
    }
    if let Some(value) = gpu.memory_total_bytes {
        metrics.push(gauge(
            "hw.gpu.memory.limit",
            "By",
            value as f64,
            time,
            attrs.clone(),
        ));
    }
    if let Some(value) = gpu.temperature_celsius {
        metrics.push(gauge(
            "hw.gpu.temperature",
            "Cel",
            value,
            time,
            attrs.clone(),
        ));
    }
    if let Some(value) = gpu.power_watts {
        metrics.push(gauge("hw.gpu.power", "W", value, time, attrs));
    }
}

fn gauge(name: &str, unit: &str, value: f64, time: u64, attributes: Vec<KeyValue>) -> Metric {
    Metric {
        name: name.into(),
        description: String::new(),
        unit: unit.into(),
        data: Some(metric::Data::Gauge(Gauge {
            data_points: vec![point(value, time, attributes)],
        })),
    }
}

fn sum(
    name: &str,
    unit: &str,
    value: u64,
    start_time: u64,
    time: u64,
    attributes: Vec<KeyValue>,
) -> Metric {
    Metric {
        name: name.into(),
        description: String::new(),
        unit: unit.into(),
        data: Some(metric::Data::Sum(Sum {
            data_points: vec![number_point(value as f64, start_time, time, attributes)],
            aggregation_temporality: AggregationTemporality::Cumulative as i32,
            is_monotonic: true,
        })),
    }
}

fn point(value: f64, time: u64, attributes: Vec<KeyValue>) -> NumberDataPoint {
    number_point(value, 0, time, attributes)
}

fn number_point(
    value: f64,
    start_time: u64,
    time: u64,
    attributes: Vec<KeyValue>,
) -> NumberDataPoint {
    NumberDataPoint {
        attributes,
        start_time_unix_nano: start_time,
        time_unix_nano: time,
        value: Some(number_data_point::Value::AsDouble(value)),
        flags: 0,
    }
}

fn attr(key: &str, value: &str) -> KeyValue {
    KeyValue {
        key: key.into(),
        value: Some(AnyValue {
            value: Some(any_value::Value::StringValue(value.into())),
        }),
    }
}

fn with_attr(attributes: &[KeyValue], key: &str, value: &str) -> Vec<KeyValue> {
    let mut values = attributes.to_vec();
    values.push(attr(key, value));
    values
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use uuid::Uuid;

    use super::*;
    use crate::model::*;

    #[test]
    fn encodes_stable_host_resource_identity() {
        let host_id = Uuid::new_v4();
        let report = AgentReport {
            schema_version: 1,
            report_id: Uuid::new_v4(),
            collected_at: Utc::now(),
            interval_seconds: 10.0,
            host: HostIdentity {
                id: host_id,
                name: "host".into(),
                os: "macos".into(),
                os_version: None,
                kernel_version: None,
                arch: "aarch64".into(),
                agent_version: "test".into(),
            },
            system: SystemSnapshot {
                uptime_seconds: 1,
                cpu: CpuSnapshot {
                    usage_percent: 10.0,
                    logical_count: 1,
                    physical_count: Some(1),
                    per_core_percent: vec![10.0],
                },
                memory: MemorySnapshot {
                    total_bytes: 2,
                    used_bytes: 1,
                    available_bytes: 1,
                    swap_total_bytes: 0,
                    swap_used_bytes: 0,
                },
                networks: vec![],
                disks: vec![],
                temperatures: vec![],
                gpus: vec![],
            },
            capabilities: vec![],
            agent: AgentHealth {
                spool_pending_batches: 0,
                collector_errors: 0,
            },
        };
        let request = encode_report(&report);
        let resource_metrics = &request.resource_metrics[0];
        let resource = resource_metrics.resource.as_ref().unwrap();
        assert_eq!(
            string_attribute(resource, "host.id"),
            Some(host_id.to_string())
        );
        assert_eq!(
            string_attribute(resource, "os.type").as_deref(),
            Some("darwin")
        );
        assert_eq!(
            string_attribute(resource, "host.arch").as_deref(),
            Some("arm64")
        );
    }

    fn string_attribute(resource: &Resource, key: &str) -> Option<String> {
        resource.attributes.iter().find_map(|attribute| {
            if attribute.key != key {
                return None;
            }
            match attribute.value.as_ref()?.value.as_ref()? {
                any_value::Value::StringValue(value) => Some(value.clone()),
                _ => None,
            }
        })
    }
}
