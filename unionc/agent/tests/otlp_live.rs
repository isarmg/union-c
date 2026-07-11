use unionc_agent::{
    AgentConfig, AgentHealth, AgentReport, CpuSnapshot, HostIdentity, MemorySnapshot,
    SystemSnapshot, transport::Reporter,
};
use uuid::Uuid;

/// CI sets UNIONC_AGENT_TEST_OTLP_ENDPOINT while a real Collector is running.
/// Local test runs skip cleanly so the unit suite has no external dependency.
#[tokio::test]
async fn collector_accepts_the_agent_otlp_protobuf() {
    let Ok(endpoint) = std::env::var("UNIONC_AGENT_TEST_OTLP_ENDPOINT") else {
        eprintln!("skipped: UNIONC_AGENT_TEST_OTLP_ENDPOINT is not configured");
        return;
    };
    let config = AgentConfig {
        token: Some("test-only-host-token".into()),
        otlp_endpoint: Some(endpoint),
        ..AgentConfig::default()
    };
    let reporter = Reporter::new(&config).expect("build OTLP test client");
    let report = AgentReport {
        schema_version: 1,
        report_id: Uuid::new_v4(),
        collected_at: chrono::Utc::now(),
        host: HostIdentity {
            id: Uuid::parse_str("00000000-0000-4000-8000-000000000001").unwrap(),
            name: "otlp-ci-host".into(),
            os: "linux".into(),
            os_version: None,
            kernel_version: None,
            arch: "x86_64".into(),
            agent_version: env!("CARGO_PKG_VERSION").into(),
        },
        interval_seconds: 10.0,
        system: SystemSnapshot {
            uptime_seconds: 60,
            cpu: CpuSnapshot {
                usage_percent: 25.0,
                logical_count: 4,
                physical_count: Some(2),
                per_core_percent: vec![10.0, 20.0, 30.0, 40.0],
            },
            memory: MemorySnapshot {
                total_bytes: 16 * 1024 * 1024,
                used_bytes: 8 * 1024 * 1024,
                available_bytes: 8 * 1024 * 1024,
                swap_total_bytes: 0,
                swap_used_bytes: 0,
            },
            networks: Vec::new(),
            disks: Vec::new(),
            temperatures: Vec::new(),
            gpus: Vec::new(),
        },
        capabilities: Vec::new(),
        agent: AgentHealth {
            spool_pending_batches: 0,
            collector_errors: 0,
        },
    };
    reporter
        .send_otlp(&report)
        .await
        .expect("Collector must accept the Agent's gzip OTLP protobuf");
}
