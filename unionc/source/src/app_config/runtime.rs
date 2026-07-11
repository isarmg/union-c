//! 启动环境覆盖、本地私有配置和目录初始化。

use std::{
    fs,
    io::Write,
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::Path,
    str::FromStr,
};

use anyhow::{Context, bail};

use super::*;

const LOCAL_CONFIG_DIR_MODE: u32 = 0o700;
const LOCAL_CONFIG_FILE_MODE: u32 = 0o600;
type LocalConfigResult<T> = Result<T, LocalConfigError>;

impl Settings {
    pub fn load() -> anyhow::Result<Self> {
        let mut settings = Settings::default();
        match load_local_config() {
            Ok(config) => settings.database.url = config.database_url,
            Err(error) if is_not_found_error(&error) => {}
            Err(error) => return Err(error.context("failed to load local bootstrap config")),
        }
        settings.apply_runtime_environment()?;
        Ok(settings)
    }

    pub fn apply_runtime_environment(&mut self) -> anyhow::Result<()> {
        self.production =
            std::env::var("UNIONC_ENV").is_ok_and(|value| value.eq_ignore_ascii_case("production"));
        if let Ok(url) =
            std::env::var("UNIONC_DATABASE_URL").or_else(|_| std::env::var("DATABASE_URL"))
        {
            self.database.url = normalize_database_url(&url)?;
        }
        if let Ok(bind) = std::env::var("UNIONC_SERVER_BIND") {
            bind.trim()
                .trim_matches(['[', ']'])
                .parse::<std::net::IpAddr>()
                .map_err(|_| anyhow::anyhow!("invalid UNIONC_SERVER_BIND"))?;
            self.server.bind = bind.trim().to_string();
        }
        if let Ok(port) = std::env::var("UNIONC_SERVER_PORT") {
            self.server.port = port
                .trim()
                .parse::<u16>()
                .map_err(|_| anyhow::anyhow!("invalid UNIONC_SERVER_PORT"))?;
            if self.server.port == 0 {
                anyhow::bail!("UNIONC_SERVER_PORT must be greater than zero");
            }
        }
        if let Ok(token) = std::env::var("UNIONC_AGENT_ENROLLMENT_TOKEN") {
            let token = token.trim();
            if token.len() < 32 || token.chars().any(char::is_whitespace) {
                anyhow::bail!(
                    "UNIONC_AGENT_ENROLLMENT_TOKEN must contain at least 32 non-whitespace characters"
                );
            }
            self.agents.enrollment_token = token.to_string();
        }
        if self.production {
            let bind = self
                .server
                .bind
                .trim()
                .trim_matches(['[', ']'])
                .parse::<std::net::IpAddr>()
                .map_err(|_| anyhow::anyhow!("invalid server bind address"))?;
            if !bind.is_loopback() {
                anyhow::bail!(
                    "production unionc must bind to a loopback address behind the reverse proxy"
                );
            }
        }
        Ok(())
    }
}

pub fn normalize_database_url(value: &str) -> LocalConfigResult<String> {
    let url = normalize_optional_database_url(value)?;
    if url.is_empty() {
        return Err(LocalConfigError::EmptyDatabaseUrl);
    }
    Ok(url)
}

pub fn load_local_config() -> anyhow::Result<LocalConfig> {
    let path = Path::new(LOCAL_CONFIG_PATH);
    ensure_private_config_file(path)?;
    let content = fs::read_to_string(path)
        .with_context(|| format!("failed to read local config {}", path.display()))?;
    let config = serde_json::from_str(&content)
        .with_context(|| format!("failed to parse local config {}", path.display()))?;
    Ok(normalize_local_config(&config)?)
}

pub fn save_local_config(config: &LocalConfig) -> anyhow::Result<()> {
    let config = normalize_local_config(config)?;
    let path = Path::new(LOCAL_CONFIG_PATH);
    let directory = path.parent().context("local config path has no parent")?;
    ensure_private_config_directory(directory)?;
    let temporary = directory.join(format!(".unionc-config.{}.tmp", uuid::Uuid::new_v4()));
    let result = write_local_config_file(&temporary, &config)
        .and_then(|()| {
            fs::rename(&temporary, path).with_context(|| "failed to replace local config")
        })
        .and_then(|()| fs::File::open(directory)?.sync_all().map_err(Into::into));
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result?;
    ensure_private_config_file(path)
}

pub fn ensure_layout(settings: &Settings) -> std::io::Result<()> {
    for dir in [
        Path::new("backc/data"),
        Path::new("unionc/data"),
        settings.paths.data_dir.as_path(),
        settings.paths.sunshine_dir.as_path(),
        settings.paths.moonlight_dir.as_path(),
    ] {
        fs::create_dir_all(dir)?;
    }
    for host in &settings.sunshine.hosts {
        if let Some(parent) = host.log_path.parent() {
            fs::create_dir_all(parent)?;
        }
    }
    for dir in [Path::new("backc/data"), Path::new("unionc/data")] {
        fs::set_permissions(dir, fs::Permissions::from_mode(LOCAL_CONFIG_DIR_MODE))?;
    }
    Ok(())
}

fn normalize_local_config(config: &LocalConfig) -> LocalConfigResult<LocalConfig> {
    let username = config.admin_username.trim();
    if username.is_empty() {
        return Err(LocalConfigError::EmptyAdminUsername);
    }
    if username.len() > 128 || username.chars().any(char::is_control) {
        return Err(LocalConfigError::InvalidAdminUsername);
    }
    let hash = config.admin_password_hash.trim();
    if hash.is_empty() {
        return Err(LocalConfigError::EmptyAdminPasswordHash);
    }
    bcrypt::HashParts::from_str(hash).map_err(|_| LocalConfigError::InvalidAdminPasswordHash)?;
    Ok(LocalConfig {
        database_url: normalize_optional_database_url(&config.database_url)?,
        admin_username: username.to_string(),
        admin_password_hash: hash.to_string(),
    })
}

fn normalize_optional_database_url(value: &str) -> LocalConfigResult<String> {
    let value = value.trim();
    if value.is_empty() {
        return Ok(String::new());
    }
    let parsed = url::Url::parse(value).map_err(|_| LocalConfigError::InvalidDatabaseUrl)?;
    if !matches!(parsed.scheme(), "postgres" | "postgresql") {
        return Err(LocalConfigError::UnsupportedDatabaseScheme);
    }
    Ok(value.to_string())
}

fn is_not_found_error(error: &anyhow::Error) -> bool {
    error
        .downcast_ref::<std::io::Error>()
        .is_some_and(|error| error.kind() == std::io::ErrorKind::NotFound)
}

fn ensure_private_config_directory(path: &Path) -> anyhow::Result<()> {
    fs::create_dir_all(path)?;
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        bail!("local config directory is not a regular directory");
    }
    fs::set_permissions(path, fs::Permissions::from_mode(LOCAL_CONFIG_DIR_MODE))?;
    Ok(())
}

fn ensure_private_config_file(path: &Path) -> anyhow::Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        bail!("local config path is not a regular file");
    }
    fs::set_permissions(path, fs::Permissions::from_mode(LOCAL_CONFIG_FILE_MODE))?;
    Ok(())
}

fn write_local_config_file(path: &Path, config: &LocalConfig) -> anyhow::Result<()> {
    let mut file = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(LOCAL_CONFIG_FILE_MODE)
        .open(path)?;
    serde_json::to_writer_pretty(&mut file, config)?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    Ok(())
}
