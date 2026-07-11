use std::{env, fs, net::IpAddr, path::PathBuf, time::Duration};

use anyhow::{Context, bail};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

const DEFAULT_ENDPOINT: &str = "http://127.0.0.1:8081/api/agent/v1/report";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentCommand {
    Run,
    Once,
    Probe,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AgentConfig {
    pub endpoint: String,
    pub registration_endpoint: Option<String>,
    /// Optional pre-provisioned per-host bearer token.
    pub token: Option<String>,
    /// One-time deployment credential used only to exchange for a per-host token.
    pub enrollment_token: Option<String>,
    pub otlp_endpoint: Option<String>,
    pub otlp_token: Option<String>,
    pub host_id: Option<Uuid>,
    pub interval_seconds: u64,
    pub slow_interval_seconds: u64,
    pub request_timeout_seconds: u64,
    pub jitter_percent: u8,
    pub state_dir: PathBuf,
    pub spool_max_bytes: u64,
    pub tls_identity_pem: Option<PathBuf>,
    pub tls_identity_pkcs12: Option<PathBuf>,
    pub tls_identity_password: Option<String>,
    pub tls_ca_pem: Option<PathBuf>,
    pub allow_insecure_http: bool,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            endpoint: DEFAULT_ENDPOINT.to_string(),
            registration_endpoint: None,
            token: None,
            enrollment_token: None,
            otlp_endpoint: None,
            otlp_token: None,
            host_id: None,
            interval_seconds: 10,
            slow_interval_seconds: 30,
            request_timeout_seconds: 10,
            jitter_percent: 10,
            state_dir: default_state_dir(),
            spool_max_bytes: 64 * 1024 * 1024,
            tls_identity_pem: None,
            tls_identity_pkcs12: None,
            tls_identity_password: None,
            tls_ca_pem: None,
            allow_insecure_http: false,
        }
    }
}

impl AgentConfig {
    pub fn load_from_args() -> anyhow::Result<(Self, AgentCommand)> {
        let mut command = AgentCommand::Run;
        let mut config_path = env::var_os("UNIONC_AGENT_CONFIG").map(PathBuf::from);
        let mut args = env::args().skip(1);
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "run" => command = AgentCommand::Run,
                "once" => command = AgentCommand::Once,
                "probe" => command = AgentCommand::Probe,
                "--config" => {
                    let value = args.next().context("--config requires a file path")?;
                    config_path = Some(PathBuf::from(value));
                }
                "-h" | "--help" => {
                    print_help();
                    std::process::exit(0);
                }
                other => bail!("unknown argument: {other}"),
            }
        }

        let mut config = if let Some(path) = config_path {
            let bytes = fs::read(&path)
                .with_context(|| format!("failed to read config {}", path.display()))?;
            serde_json::from_slice(&bytes)
                .with_context(|| format!("failed to parse config {}", path.display()))?
        } else {
            Self::default()
        };
        config.apply_environment()?;
        config.validate(command)?;
        Ok((config, command))
    }

    fn apply_environment(&mut self) -> anyhow::Result<()> {
        if let Ok(value) = env::var("UNIONC_AGENT_ENDPOINT") {
            self.endpoint = value;
        }
        if let Ok(value) = env::var("UNIONC_AGENT_TOKEN") {
            self.token = non_empty(value);
        }
        if let Ok(value) = env::var("UNIONC_AGENT_ENROLLMENT_TOKEN") {
            self.enrollment_token = non_empty(value);
        }
        if let Ok(value) = env::var("UNIONC_AGENT_REGISTRATION_ENDPOINT") {
            self.registration_endpoint = non_empty(value);
        }
        if let Ok(value) = env::var("UNIONC_AGENT_OTLP_ENDPOINT") {
            self.otlp_endpoint = non_empty(value);
        }
        if let Ok(value) = env::var("UNIONC_AGENT_OTLP_TOKEN") {
            self.otlp_token = non_empty(value);
        }
        if let Ok(value) = env::var("UNIONC_AGENT_HOST_ID") {
            self.host_id = Some(value.parse().context("invalid UNIONC_AGENT_HOST_ID")?);
        }
        if let Ok(value) = env::var("UNIONC_AGENT_STATE_DIR") {
            self.state_dir = PathBuf::from(value);
        }
        if let Ok(value) = env::var("UNIONC_AGENT_INTERVAL_SECONDS") {
            self.interval_seconds = value.parse().context("invalid interval")?;
        }
        if let Ok(value) = env::var("UNIONC_AGENT_SLOW_INTERVAL_SECONDS") {
            self.slow_interval_seconds = value.parse().context("invalid slow interval")?;
        }
        if let Ok(value) = env::var("UNIONC_AGENT_TLS_IDENTITY_PEM") {
            self.tls_identity_pem = Some(PathBuf::from(value));
        }
        if let Ok(value) = env::var("UNIONC_AGENT_TLS_IDENTITY_PKCS12") {
            self.tls_identity_pkcs12 = Some(PathBuf::from(value));
        }
        if let Ok(value) = env::var("UNIONC_AGENT_TLS_IDENTITY_PASSWORD") {
            self.tls_identity_password = Some(value);
        }
        if let Ok(value) = env::var("UNIONC_AGENT_TLS_CA_PEM") {
            self.tls_ca_pem = Some(PathBuf::from(value));
        }
        if let Ok(value) = env::var("UNIONC_AGENT_ALLOW_INSECURE_HTTP") {
            self.allow_insecure_http =
                parse_bool(&value).context("invalid UNIONC_AGENT_ALLOW_INSECURE_HTTP boolean")?;
        }
        Ok(())
    }

    fn validate(&self, command: AgentCommand) -> anyhow::Result<()> {
        if self.interval_seconds == 0 {
            bail!("interval_seconds must be greater than zero");
        }
        if self.slow_interval_seconds < self.interval_seconds {
            bail!("slow_interval_seconds must be at least interval_seconds");
        }
        if self.jitter_percent > 50 {
            bail!("jitter_percent must not exceed 50");
        }
        if self.request_timeout_seconds == 0 {
            bail!("request_timeout_seconds must be greater than zero");
        }
        if self.spool_max_bytes < 1024 * 1024 {
            bail!("spool_max_bytes must be at least 1 MiB");
        }
        validate_endpoint(&self.endpoint, self.allow_insecure_http)?;
        validate_endpoint(&self.registration_endpoint(), self.allow_insecure_http)?;
        if let Some(endpoint) = &self.otlp_endpoint {
            validate_endpoint(endpoint, self.allow_insecure_http)?;
        }
        if self.tls_identity_pem.is_some() && self.tls_identity_pkcs12.is_some() {
            bail!("configure only one TLS client identity format");
        }
        if command != AgentCommand::Probe
            && self.token.as_deref().unwrap_or("").is_empty()
            && self.enrollment_token.as_deref().unwrap_or("").is_empty()
            && !self.state_dir.join("agent-token").is_file()
        {
            bail!(
                "a host token or enrollment token is required; set UNIONC_AGENT_TOKEN, \
                 UNIONC_AGENT_ENROLLMENT_TOKEN, or enroll this state directory first"
            );
        }
        Ok(())
    }

    pub fn interval(&self) -> Duration {
        Duration::from_secs(self.interval_seconds)
    }

    pub fn request_timeout(&self) -> Duration {
        Duration::from_secs(self.request_timeout_seconds)
    }

    pub fn registration_endpoint(&self) -> String {
        self.registration_endpoint.clone().unwrap_or_else(|| {
            self.endpoint
                .strip_suffix("/report")
                .map(|base| format!("{base}/register"))
                .unwrap_or_else(|| format!("{}/register", self.endpoint.trim_end_matches('/')))
        })
    }
}

fn non_empty(value: String) -> Option<String> {
    let value = value.trim().to_string();
    (!value.is_empty()).then_some(value)
}

fn parse_bool(value: &str) -> anyhow::Result<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Ok(true),
        "0" | "false" | "no" | "off" => Ok(false),
        _ => bail!("expected true/false, 1/0, yes/no, or on/off"),
    }
}

fn validate_endpoint(endpoint: &str, allow_insecure_http: bool) -> anyhow::Result<()> {
    let url = reqwest::Url::parse(endpoint)
        .with_context(|| format!("invalid telemetry endpoint {endpoint}"))?;
    if !url.username().is_empty() || url.password().is_some() {
        bail!("telemetry endpoint must not embed credentials");
    }
    match url.scheme() {
        "https" => Ok(()),
        "http" if allow_insecure_http || is_loopback_host(url.host_str()) => Ok(()),
        "http" => bail!(
            "plain HTTP telemetry is allowed only for loopback; use HTTPS or explicitly set \
             allow_insecure_http for an isolated trusted network"
        ),
        scheme => bail!("unsupported telemetry endpoint scheme: {scheme}"),
    }
}

fn is_loopback_host(host: Option<&str>) -> bool {
    host.is_some_and(|host| {
        host.eq_ignore_ascii_case("localhost")
            || host
                .parse::<IpAddr>()
                .is_ok_and(|address| address.is_loopback())
    })
}

fn default_state_dir() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        let base = env::var_os("PROGRAMDATA").unwrap_or_else(|| "C:\\ProgramData".into());
        return PathBuf::from(base).join("UnionC Agent");
    }
    #[cfg(target_os = "macos")]
    {
        return PathBuf::from("/Library/Application Support/UnionC Agent");
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        PathBuf::from("/var/lib/unionc-agent")
    }
}

fn print_help() {
    println!(
        "unionc-agent [run|once|probe] [--config PATH]\n\
         run   continuously collect and report read-only telemetry (default)\n\
         once  collect and report one snapshot\n\
         probe print the local capability report without contacting a server\n\n\
         First start: set UNIONC_AGENT_ENROLLMENT_TOKEN. The exchanged per-host token is\n\
         stored in the state directory and the deployment token is not persisted."
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derives_registration_endpoint_from_report_endpoint() {
        let config = AgentConfig::default();
        assert_eq!(
            config.registration_endpoint(),
            "http://127.0.0.1:8081/api/agent/v1/register"
        );
    }

    #[test]
    fn rejects_remote_plaintext_by_default() {
        assert!(validate_endpoint("http://192.0.2.10/report", false).is_err());
        assert!(validate_endpoint("http://127.0.0.1/report", false).is_ok());
        assert!(validate_endpoint("https://telemetry.example/report", false).is_ok());
    }
}
