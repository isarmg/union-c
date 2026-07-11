use std::{fs, io::Write, path::Path};

use anyhow::{Context, bail};
use flate2::{Compression, write::GzEncoder};
use reqwest::{Certificate, Client, Identity, StatusCode};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    config::AgentConfig,
    model::{AgentReport, HostIdentity},
};

#[derive(Clone)]
pub struct Reporter {
    client: Client,
    endpoint: String,
    token: String,
    otlp_endpoint: Option<String>,
    otlp_token: Option<String>,
}

impl Reporter {
    pub fn new(config: &AgentConfig) -> anyhow::Result<Self> {
        let token = config
            .token
            .clone()
            .context("a per-host token is required before creating the reporter")?;
        Self::with_token(config, token)
    }

    /// Load the previously enrolled token, use a pre-provisioned host token, or
    /// exchange the deployment enrollment token for a host-scoped credential.
    pub async fn for_host(config: &AgentConfig, host: &HostIdentity) -> anyhow::Result<Self> {
        let token_path = config.state_dir.join("agent-token");
        if let Some(token) = config.token.clone() {
            return Self::with_token(config, token);
        }
        if token_path.is_file() {
            return Self::with_token(config, read_secret(&token_path, "host token")?);
        }

        let enrollment_token = config
            .enrollment_token
            .as_deref()
            .context("no per-host token or enrollment token is available")?;
        let enrollment_secret_path = config.state_dir.join("enrollment-secret");
        let enrollment_secret = if enrollment_secret_path.is_file() {
            read_secret(&enrollment_secret_path, "enrollment secret")?
        } else {
            let secret = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
            persist_private_value(&enrollment_secret_path, &secret, "enrollment secret")?;
            secret
        };
        let client = build_client(config)?;
        let response = client
            .post(config.registration_endpoint())
            .bearer_auth(enrollment_token)
            .json(&EnrollmentRequest {
                host,
                enrollment_secret: &enrollment_secret,
            })
            .send()
            .await?;
        let status = response.status();
        let body = response.bytes().await?;
        if !status.is_success() {
            ensure_success(
                status,
                String::from_utf8_lossy(&body).into_owned(),
                "UnionC enrollment",
            )?;
        }
        let enrollment: EnrollmentResponse = serde_json::from_slice(&body)
            .context("UnionC returned an invalid enrollment response")?;
        if enrollment.host_id != host.id {
            bail!(
                "UnionC enrollment returned host id {}, expected {}",
                enrollment.host_id,
                host.id
            );
        }
        persist_private_value(&token_path, &enrollment.token, "host token")?;
        Self::with_client_and_token(config, client, enrollment.token)
    }

    fn with_token(config: &AgentConfig, token: String) -> anyhow::Result<Self> {
        let client = build_client(config)?;
        Self::with_client_and_token(config, client, token)
    }

    fn with_client_and_token(
        config: &AgentConfig,
        client: Client,
        token: String,
    ) -> anyhow::Result<Self> {
        if token.trim().is_empty() {
            bail!("the per-host token is empty");
        }
        Ok(Self {
            client,
            endpoint: config.endpoint.clone(),
            token,
            otlp_endpoint: config.otlp_endpoint.clone(),
            otlp_token: config.otlp_token.clone(),
        })
    }

    pub async fn send_unionc(&self, report: &AgentReport) -> anyhow::Result<()> {
        let response = self
            .client
            .post(&self.endpoint)
            .bearer_auth(&self.token)
            .json(report)
            .send()
            .await?;
        ensure_success(
            response.status(),
            response.text().await.unwrap_or_default(),
            "UnionC",
        )
    }

    #[cfg(feature = "otlp")]
    pub async fn send_otlp(&self, report: &AgentReport) -> anyhow::Result<()> {
        use prost::Message;

        let Some(endpoint) = &self.otlp_endpoint else {
            return Ok(());
        };
        let request = crate::otlp::encode_report(report);
        let mut protobuf = Vec::with_capacity(request.encoded_len());
        request.encode(&mut protobuf)?;
        let mut encoder = GzEncoder::new(Vec::new(), Compression::fast());
        encoder.write_all(&protobuf)?;
        let body = encoder.finish()?;
        let mut request = self
            .client
            .post(endpoint)
            .header("content-type", "application/x-protobuf")
            .header("content-encoding", "gzip")
            .body(body);
        if let Some(token) = &self.otlp_token {
            request = request.bearer_auth(token);
        }
        let response = request.send().await?;
        ensure_success(
            response.status(),
            response.text().await.unwrap_or_default(),
            "OTLP",
        )
    }

    #[cfg(not(feature = "otlp"))]
    pub async fn send_otlp(&self, _report: &AgentReport) -> anyhow::Result<()> {
        Ok(())
    }
}

#[derive(Deserialize)]
struct EnrollmentResponse {
    host_id: Uuid,
    #[serde(alias = "agent_token")]
    token: String,
}

#[derive(Serialize)]
struct EnrollmentRequest<'a> {
    #[serde(flatten)]
    host: &'a HostIdentity,
    enrollment_secret: &'a str,
}

fn build_client(config: &AgentConfig) -> anyhow::Result<Client> {
    let mut builder = Client::builder()
        .timeout(config.request_timeout())
        .user_agent(format!("unionc-agent/{}", env!("CARGO_PKG_VERSION")));
    #[cfg(all(not(windows), not(target_os = "macos")))]
    if let Some(path) = &config.tls_identity_pem {
        let bytes = fs::read(path)
            .with_context(|| format!("failed to read TLS identity {}", path.display()))?;
        builder = builder.identity(Identity::from_pem(&bytes)?);
    }
    #[cfg(any(windows, target_os = "macos"))]
    {
        if config.tls_identity_pem.is_some() {
            bail!(
                "the native TLS backend requires tls_identity_pkcs12 instead of tls_identity_pem"
            );
        }
        if let Some(path) = &config.tls_identity_pkcs12 {
            let bytes = fs::read(path)
                .with_context(|| format!("failed to read TLS identity {}", path.display()))?;
            builder = builder.identity(Identity::from_pkcs12_der(
                &bytes,
                config.tls_identity_password.as_deref().unwrap_or(""),
            )?);
        }
    }
    if let Some(path) = &config.tls_ca_pem {
        let bytes =
            fs::read(path).with_context(|| format!("failed to read TLS CA {}", path.display()))?;
        builder = builder.add_root_certificate(Certificate::from_pem(&bytes)?);
    }
    Ok(builder.build()?)
}

fn read_secret(path: &Path, kind: &str) -> anyhow::Result<String> {
    let token = fs::read_to_string(path)
        .with_context(|| format!("failed to read {kind} {}", path.display()))?;
    let token = token.trim().to_string();
    if token.is_empty() {
        bail!("{kind} {} is empty", path.display());
    }
    Ok(token)
}

fn persist_private_value(path: &Path, token: &str, kind: &str) -> anyhow::Result<()> {
    if token.trim().is_empty() {
        bail!("refusing to persist an empty {kind}");
    }
    let parent = path
        .parent()
        .context("token path has no parent directory")?;
    fs::create_dir_all(parent)?;
    set_private_directory_permissions(parent)?;
    let temporary = parent.join(format!(".agent-token-{}.tmp", Uuid::new_v4()));

    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let result = (|| -> anyhow::Result<()> {
        let mut file = options.open(&temporary)?;
        file.write_all(token.trim().as_bytes())?;
        file.sync_all()?;
        fs::rename(&temporary, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result.with_context(|| format!("failed to persist {kind} {}", path.display()))
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

fn ensure_success(status: StatusCode, body: String, target: &str) -> anyhow::Result<()> {
    if status.is_success() {
        return Ok(());
    }
    let detail: String = body.chars().take(512).collect();
    bail!("{target} rejected telemetry with HTTP {status}: {detail}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn persists_trimmed_host_token() {
        let directory = std::env::temp_dir().join(format!("unionc-agent-token-{}", Uuid::new_v4()));
        let path = directory.join("agent-token");
        persist_private_value(&path, " secret-token\n", "host token").unwrap();
        assert_eq!(read_secret(&path, "host token").unwrap(), "secret-token");

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
            assert_eq!(
                fs::metadata(&directory).unwrap().permissions().mode() & 0o777,
                0o700
            );
        }
        fs::remove_dir_all(directory).unwrap();
    }
}
