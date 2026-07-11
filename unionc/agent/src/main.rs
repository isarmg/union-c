use std::time::{Duration, Instant};

use anyhow::Context;
use rand::random;
use tokio::sync::mpsc;
use tracing::{error, info, warn};
use unionc_agent::{
    AgentCommand, AgentConfig, SystemSampler, collectors::load_or_create_host_identity,
    model::AgentReport, spool::Spool, transport::Reporter,
};
use uuid::Uuid;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "unionc_agent=info".into()),
        )
        .init();

    let (config, command) = AgentConfig::load_from_args()?;
    let configured_id = if command == AgentCommand::Probe && config.host_id.is_none() {
        Some(Uuid::new_v4())
    } else {
        config.host_id
    };
    let host = load_or_create_host_identity(&config.state_dir, configured_id)?;
    let mut sampler = SystemSampler::new();
    tokio::time::sleep(sysinfo::MINIMUM_CPU_UPDATE_INTERVAL).await;

    if command == AgentCommand::Probe {
        let report = sampler.collect(host, config.slow_interval_seconds, 0);
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }

    let spool = Spool::open(&config.state_dir, config.spool_max_bytes)
        .with_context(|| format!("failed to open spool in {}", config.state_dir.display()))?;
    let Some(reporter) = prepare_reporter(&config, &host, command).await? else {
        info!("shutdown signal received while waiting to enroll");
        return Ok(());
    };

    if command == AgentCommand::Once {
        let report = sampler.collect(host, config.slow_interval_seconds, spool.pending_count()?);
        if let Err(error) = reporter.send_unionc(&report).await {
            spool.enqueue(&report)?;
            return Err(error.context("report was retained in the local spool"));
        }
        if let Err(error) = reporter.send_otlp(&report).await {
            warn!("optional OTLP export failed: {error}");
        }
        return Ok(());
    }

    let otlp_queue = config
        .otlp_endpoint
        .as_ref()
        .map(|_| OtlpQueue::spawn(reporter.clone()));
    info!(host_id = %host.id, host_name = %host.name, "read-only telemetry agent started");
    run_loop(config, host, sampler, spool, reporter, otlp_queue).await
}

#[derive(Clone)]
struct OtlpQueue {
    sender: mpsc::Sender<AgentReport>,
}

impl OtlpQueue {
    fn spawn(reporter: Reporter) -> Self {
        // OTLP is an optional secondary output. A bounded worker prevents a slow
        // collector from delaying host sampling or primary UnionC delivery.
        let (sender, mut receiver) = mpsc::channel::<AgentReport>(128);
        tokio::spawn(async move {
            while let Some(report) = receiver.recv().await {
                if let Err(error) = reporter.send_otlp(&report).await {
                    warn!(report_id = %report.report_id, "optional OTLP export failed: {error}");
                }
            }
        });
        Self { sender }
    }

    fn try_export(&self, report: &AgentReport) {
        if let Err(error) = self.sender.try_send(report.clone()) {
            warn!(report_id = %report.report_id, "optional OTLP queue rejected a report: {error}");
        }
    }
}

async fn prepare_reporter(
    config: &AgentConfig,
    host: &unionc_agent::HostIdentity,
    command: AgentCommand,
) -> anyhow::Result<Option<Reporter>> {
    let mut backoff = Duration::from_secs(1);
    loop {
        match Reporter::for_host(config, host).await {
            Ok(reporter) => return Ok(Some(reporter)),
            Err(error) if command != AgentCommand::Run => {
                return Err(error.context("failed to enroll or load the host credential"));
            }
            Err(error) => {
                let delay = jitter(backoff, config.jitter_percent);
                warn!(
                    retry_seconds = delay.as_secs_f64(),
                    "host enrollment failed; retrying with bounded backoff: {error}"
                );
                tokio::select! {
                    _ = tokio::time::sleep(delay) => {},
                    result = shutdown_signal() => {
                        result?;
                        return Ok(None);
                    }
                }
                backoff = (backoff * 2).min(Duration::from_secs(300));
            }
        }
    }
}

async fn run_loop(
    config: AgentConfig,
    host: unionc_agent::HostIdentity,
    mut sampler: SystemSampler,
    spool: Spool,
    reporter: Reporter,
    otlp_queue: Option<OtlpQueue>,
) -> anyhow::Result<()> {
    let mut retry_at = Instant::now();
    let mut backoff = Duration::from_secs(1);

    enum Delivery {
        Succeeded,
        Deferred,
        Failed(anyhow::Error),
    }

    loop {
        let pending = spool.pending_count()?;
        let report = sampler.collect(host.clone(), config.slow_interval_seconds, pending);

        let delivery = if pending == 0 && Instant::now() >= retry_at {
            match reporter.send_unionc(&report).await {
                Ok(()) => {
                    if let Some(queue) = &otlp_queue {
                        queue.try_export(&report);
                    }
                    Delivery::Succeeded
                }
                Err(error) => {
                    spool.enqueue(&report)?;
                    Delivery::Failed(error)
                }
            }
        } else {
            spool.enqueue(&report)?;
            if Instant::now() >= retry_at {
                match flush_spool(&spool, &reporter, otlp_queue.as_ref()).await {
                    Ok(()) => Delivery::Succeeded,
                    Err(error) => Delivery::Failed(error),
                }
            } else {
                Delivery::Deferred
            }
        };

        match delivery {
            Delivery::Succeeded => {
                retry_at = Instant::now();
                backoff = Duration::from_secs(1);
            }
            Delivery::Deferred => {}
            Delivery::Failed(error) => {
                warn!(
                    pending = spool.pending_count().unwrap_or(0),
                    "telemetry delivery failed: {error}"
                );
                retry_at = Instant::now() + jitter(backoff, config.jitter_percent);
                backoff = (backoff * 2).min(Duration::from_secs(300));
            }
        }

        let sleep = jitter(config.interval(), config.jitter_percent);
        tokio::select! {
            _ = tokio::time::sleep(sleep) => {},
            result = shutdown_signal() => {
                if let Err(error) = result { error!("shutdown handler failed: {error}"); }
                info!("shutdown signal received");
                return Ok(());
            }
        }
    }
}

async fn flush_spool(
    spool: &Spool,
    reporter: &Reporter,
    otlp_queue: Option<&OtlpQueue>,
) -> anyhow::Result<()> {
    // 每轮最多补传 32 个批次，避免长时间断线恢复后独占网络和采样线程。
    for _ in 0..32 {
        let Some(pending) = spool.oldest()? else {
            return Ok(());
        };
        reporter.send_unionc(&pending.report).await?;
        if let Some(queue) = otlp_queue {
            queue.try_export(&pending.report);
        }
        spool.acknowledge(pending)?;
    }
    Ok(())
}

fn jitter(base: Duration, percent: u8) -> Duration {
    if percent == 0 {
        return base;
    }
    let range = percent as f64 / 100.0;
    let factor = (1.0 - range) + random::<f64>() * range * 2.0;
    Duration::from_secs_f64((base.as_secs_f64() * factor).max(0.05))
}

#[cfg(unix)]
async fn shutdown_signal() -> anyhow::Result<()> {
    let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
    tokio::select! {
        result = tokio::signal::ctrl_c() => result?,
        _ = terminate.recv() => {},
    }
    Ok(())
}

#[cfg(not(unix))]
async fn shutdown_signal() -> anyhow::Result<()> {
    tokio::signal::ctrl_c().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_jitter_is_exact() {
        assert_eq!(jitter(Duration::from_secs(10), 0), Duration::from_secs(10));
    }
}
