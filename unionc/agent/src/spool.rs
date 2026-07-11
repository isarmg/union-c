use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
};

use uuid::Uuid;

use crate::model::AgentReport;

#[derive(Debug, Clone)]
pub struct Spool {
    directory: PathBuf,
    max_bytes: u64,
}

#[derive(Debug)]
pub struct PendingReport {
    path: PathBuf,
    pub report: AgentReport,
}

impl Spool {
    pub fn open(state_dir: &Path, max_bytes: u64) -> io::Result<Self> {
        let directory = state_dir.join("spool");
        fs::create_dir_all(&directory)?;
        set_private_directory_permissions(&directory)?;
        Ok(Self {
            directory,
            max_bytes,
        })
    }

    pub fn pending_count(&self) -> io::Result<u64> {
        Ok(self.paths()?.len() as u64)
    }

    pub fn enqueue(&self, report: &AgentReport) -> anyhow::Result<()> {
        let timestamp = report.collected_at.timestamp_millis().max(0);
        let name = format!("{timestamp:020}-{}.json", report.report_id);
        let target = self.directory.join(name);
        let temporary = self.directory.join(format!(".{}.tmp", Uuid::new_v4()));
        let bytes = serde_json::to_vec(report)?;
        let result = (|| -> anyhow::Result<()> {
            let mut options = fs::OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            let mut file = options.open(&temporary)?;
            file.write_all(&bytes)?;
            file.sync_all()?;
            fs::rename(&temporary, target)?;
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result?;
        self.enforce_limit()?;
        Ok(())
    }

    pub fn oldest(&self) -> anyhow::Result<Option<PendingReport>> {
        let Some(path) = self.paths()?.into_iter().next() else {
            return Ok(None);
        };
        let bytes = fs::read(&path)?;
        match serde_json::from_slice(&bytes) {
            Ok(report) => Ok(Some(PendingReport { path, report })),
            Err(error) => {
                let quarantine = path.with_extension("invalid");
                let _ = fs::rename(&path, quarantine);
                Err(error.into())
            }
        }
    }

    pub fn acknowledge(&self, pending: PendingReport) -> io::Result<()> {
        fs::remove_file(pending.path)
    }

    fn paths(&self) -> io::Result<Vec<PathBuf>> {
        let mut paths = Vec::new();
        for entry in fs::read_dir(&self.directory)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().is_some_and(|value| value == "json") {
                paths.push(path);
            }
        }
        paths.sort();
        Ok(paths)
    }

    fn enforce_limit(&self) -> io::Result<()> {
        let paths = self.paths()?;
        let mut total = paths.iter().try_fold(0_u64, |total, path| {
            fs::metadata(path).map(|metadata| total.saturating_add(metadata.len()))
        })?;
        for path in paths {
            if total <= self.max_bytes {
                break;
            }
            let size = fs::metadata(&path).map(|value| value.len()).unwrap_or(0);
            fs::remove_file(path)?;
            total = total.saturating_sub(size);
        }
        Ok(())
    }
}

#[cfg(unix)]
fn set_private_directory_permissions(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
}

#[cfg(not(unix))]
fn set_private_directory_permissions(_path: &Path) -> io::Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use chrono::Utc;

    use super::*;
    use crate::model::*;

    fn report() -> AgentReport {
        AgentReport {
            schema_version: 1,
            report_id: Uuid::new_v4(),
            collected_at: Utc::now(),
            host: HostIdentity {
                id: Uuid::new_v4(),
                name: "test".into(),
                os: "test".into(),
                os_version: None,
                kernel_version: None,
                arch: "test".into(),
                agent_version: "test".into(),
            },
            interval_seconds: 10.0,
            system: SystemSnapshot {
                uptime_seconds: 1,
                cpu: CpuSnapshot {
                    usage_percent: 1.0,
                    logical_count: 1,
                    physical_count: Some(1),
                    per_core_percent: vec![1.0],
                },
                memory: MemorySnapshot {
                    total_bytes: 1,
                    used_bytes: 1,
                    available_bytes: 0,
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
        }
    }

    #[test]
    fn persists_and_acknowledges_in_order() {
        let path = std::env::temp_dir().join(format!("unionc-agent-spool-{}", Uuid::new_v4()));
        let spool = Spool::open(&path, 1024 * 1024).unwrap();
        spool.enqueue(&report()).unwrap();
        assert_eq!(spool.pending_count().unwrap(), 1);
        let pending = spool.oldest().unwrap().unwrap();
        spool.acknowledge(pending).unwrap();
        assert_eq!(spool.pending_count().unwrap(), 0);
        fs::remove_dir_all(path).unwrap();
    }
}
