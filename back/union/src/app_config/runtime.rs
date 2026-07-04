//! 启动环境覆盖、本地私有配置和目录初始化。

use std::{
    io::Write,
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    str::FromStr,
};

use anyhow::{Context, bail};

use super::*;

const LOCAL_CONFIG_DIR_MODE: u32 = 0o700;
const LOCAL_CONFIG_FILE_MODE: u32 = 0o600;
const GROUP_OR_WORLD_PERMISSION_BITS: u32 = 0o077;
type LocalConfigResult<T> = Result<T, LocalConfigError>;

impl Settings {
    /// 加载配置。
    ///
    /// 这里只允许读取 PostgreSQL 启动连接串；其它运行配置统一从 PostgreSQL 的 `settings` 表读取。
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

    /// 把只能由部署环境决定的安全配置应用到数据库配置之上。
    pub fn apply_runtime_environment(&mut self) -> anyhow::Result<()> {
        self.production =
            env::var("UNION_ENV").is_ok_and(|value| value.eq_ignore_ascii_case("production"));

        if let Ok(url) = env::var("UNION_DATABASE_URL").or_else(|_| env::var("DATABASE_URL")) {
            self.database.url = normalize_database_url(&url)?;
        }
        self.ram.public_url = env::var("UNION_RAM_PUBLIC_URL")
            .ok()
            .map(|value| value.trim_end_matches('/').to_string())
            .filter(|value| !value.is_empty());
        if self.production {
            let bind = parse_bind_ip(&self.server.bind, "server")?;
            if !bind.is_loopback() {
                anyhow::bail!(
                    "production union must bind to a loopback address behind the reverse proxy"
                );
            }

            let ram_bind = parse_bind_ip(&self.ram.bind, "ram")?;
            if !ram_bind.is_loopback() {
                anyhow::bail!("production ram must bind to a loopback address");
            }
            if self
                .ram
                .public_url
                .as_deref()
                .is_none_or(|url| !url.starts_with("https://"))
            {
                anyhow::bail!("UNION_RAM_PUBLIC_URL must be an https:// URL in production");
            }
            if has_known_default_ram_credentials(&self.ram) {
                anyhow::bail!("production refuses known default ram credentials");
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
    let directory = local_config_directory(path)?;
    ensure_private_config_directory(directory)?;

    let temporary = temporary_config_path(path)?;
    let result = write_local_config_file(&temporary, &config)
        .and_then(|()| {
            fs::rename(&temporary, path).with_context(|| {
                format!(
                    "failed to replace local config {} with {}",
                    path.display(),
                    temporary.display()
                )
            })
        })
        .and_then(|()| sync_directory(directory));

    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result?;
    ensure_private_config_file(path)?;
    Ok(())
}

/// 确保项目运行需要的目录全部存在。
///
/// `create_dir_all` 类似命令行里的 `mkdir -p`：目录已经存在不会报错，父目录不存在会一起创建。
pub fn ensure_layout(settings: &Settings) -> std::io::Result<()> {
    if settings.production {
        ensure_production_artifacts()?;
    }

    // 首次启动时补齐目录结构，避免后续写日志、写文章或启动 ram 时才失败。
    for dir in runtime_directories(settings) {
        fs::create_dir_all(dir)?;
    }

    create_parent_dir(&settings.ram.log_path)?;
    create_parent_dir(&settings.ram.process_log_path)?;
    for host in &settings.sunshine.hosts {
        create_parent_dir(&host.log_path)?;
    }
    fs::create_dir_all(&settings.blog.build_log_dir)?;

    // 运行数据不应被同机其他账号遍历。构建产物不在 data/ 下，不受此限制。
    fs::set_permissions("data", fs::Permissions::from_mode(LOCAL_CONFIG_DIR_MODE))?;

    Ok(())
}

fn parse_bind_ip(value: &str, label: &str) -> anyhow::Result<IpAddr> {
    value
        .trim()
        .trim_matches(['[', ']'])
        .parse()
        .map_err(|_| anyhow::anyhow!("invalid {label} bind address"))
}

fn has_known_default_ram_credentials(settings: &RamSettings) -> bool {
    settings
        .auth
        .iter()
        .any(|rule| rule.contains("change-me") || rule.contains("guest:guest"))
        || settings
            .management_auth
            .as_deref()
            .is_some_and(|value| value.contains("change-me"))
}

fn normalize_local_config(config: &LocalConfig) -> LocalConfigResult<LocalConfig> {
    let admin_username = normalize_admin_username(&config.admin_username)?;
    let admin_password_hash = normalize_admin_password_hash(&config.admin_password_hash)?;
    Ok(LocalConfig {
        database_url: normalize_optional_database_url(&config.database_url)?,
        admin_username,
        admin_password_hash,
    })
}

fn normalize_optional_database_url(value: &str) -> LocalConfigResult<String> {
    let url = value.trim();
    if url.is_empty() {
        return Ok(String::new());
    }
    let parsed = url::Url::parse(url).map_err(|_| LocalConfigError::InvalidDatabaseUrl)?;
    if !matches!(parsed.scheme(), "postgres" | "postgresql") {
        return Err(LocalConfigError::UnsupportedDatabaseScheme);
    }
    Ok(url.to_string())
}

fn normalize_admin_username(value: &str) -> LocalConfigResult<String> {
    let username = value.trim();
    if username.is_empty() {
        return Err(LocalConfigError::EmptyAdminUsername);
    }
    if username.len() > 128 || username.chars().any(char::is_control) {
        return Err(LocalConfigError::InvalidAdminUsername);
    }
    Ok(username.to_string())
}

fn normalize_admin_password_hash(value: &str) -> LocalConfigResult<String> {
    let hash = value.trim();
    if hash.is_empty() {
        return Err(LocalConfigError::EmptyAdminPasswordHash);
    }
    bcrypt::HashParts::from_str(hash).map_err(|_| LocalConfigError::InvalidAdminPasswordHash)?;
    Ok(hash.to_string())
}

fn is_not_found_error(error: &anyhow::Error) -> bool {
    error
        .downcast_ref::<std::io::Error>()
        .is_some_and(|error| error.kind() == std::io::ErrorKind::NotFound)
}

fn ensure_private_config_directory(path: &Path) -> anyhow::Result<()> {
    fs::create_dir_all(path)
        .with_context(|| format!("failed to create local config directory {}", path.display()))?;
    let metadata = fs::symlink_metadata(path).with_context(|| {
        format!(
            "failed to inspect local config directory {}",
            path.display()
        )
    })?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        bail!(
            "local config directory is not a regular directory: {}",
            path.display()
        );
    }
    fs::set_permissions(path, fs::Permissions::from_mode(LOCAL_CONFIG_DIR_MODE)).with_context(
        || {
            format!(
                "failed to protect local config directory {}",
                path.display()
            )
        },
    )
}

fn ensure_private_config_file(path: &Path) -> anyhow::Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        bail!(
            "local config path is not a regular file: {}",
            path.display()
        );
    }
    let mode = metadata.permissions().mode() & 0o777;
    if mode != LOCAL_CONFIG_FILE_MODE {
        if mode & GROUP_OR_WORLD_PERMISSION_BITS != 0 {
            tracing::warn!(
                "tightening group/world permissions on local config {}",
                path.display()
            );
        }
        fs::set_permissions(path, fs::Permissions::from_mode(LOCAL_CONFIG_FILE_MODE))
            .with_context(|| format!("failed to protect local config {}", path.display()))?;
    }
    Ok(())
}

fn local_config_directory(path: &Path) -> anyhow::Result<&Path> {
    path.parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .context("local config path has no parent directory")
}

fn temporary_config_path(path: &Path) -> anyhow::Result<PathBuf> {
    let directory = local_config_directory(path)?;
    Ok(directory.join(format!(".union-config.{}.tmp", uuid::Uuid::new_v4())))
}

fn write_local_config_file(path: &Path, config: &LocalConfig) -> anyhow::Result<()> {
    let mut file = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(LOCAL_CONFIG_FILE_MODE)
        .open(path)
        .with_context(|| format!("failed to create temporary local config {}", path.display()))?;
    serde_json::to_writer_pretty(&mut file, config)
        .with_context(|| format!("failed to serialize local config {}", path.display()))?;
    file.write_all(b"\n")
        .with_context(|| format!("failed to finish local config {}", path.display()))?;
    file.sync_all()
        .with_context(|| format!("failed to sync local config {}", path.display()))?;
    fs::set_permissions(path, fs::Permissions::from_mode(LOCAL_CONFIG_FILE_MODE)).with_context(
        || {
            format!(
                "failed to protect temporary local config {}",
                path.display()
            )
        },
    )
}

fn sync_directory(path: &Path) -> anyhow::Result<()> {
    fs::File::open(path)
        .with_context(|| format!("failed to open directory {}", path.display()))?
        .sync_all()
        .with_context(|| format!("failed to sync directory {}", path.display()))
}

fn ensure_production_artifacts() -> std::io::Result<()> {
    for required in [
        Path::new("back/blog/package.json"),
        Path::new("back/blog/dist/index.html"),
        Path::new("back/back/dist/index.html"),
    ] {
        if !required.is_file() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!(
                    "required production artifact is missing: {}",
                    required.display()
                ),
            ));
        }
    }
    Ok(())
}

fn runtime_directories(settings: &Settings) -> [&Path; 11] {
    [
        Path::new("data"),
        settings.paths.sunshine_dir.as_path(),
        settings.paths.moonlight_dir.as_path(),
        settings.paths.data_dir.as_path(),
        settings.paths.public_dir.as_path(),
        settings.paths.inbox_dir.as_path(),
        settings.paths.private_dir.as_path(),
        settings.paths.blog_export_dir.as_path(),
        settings.paths.blog_assets_dir.as_path(),
        settings.paths.media_dir.as_path(),
        settings.paths.logs_dir.as_path(),
    ]
}

fn create_parent_dir(path: &Path) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID_BCRYPT_HASH: &str = "$2b$12$9pdMQ6jDj0NeB.BOYV/FIeGLgLOG0hmUI/gUCWeXXI9xnasB4KbKa";

    #[test]
    fn normalizes_local_config_values() {
        let config = LocalConfig {
            database_url: " postgresql://union:secret@127.0.0.1:5432/union ".to_string(),
            admin_username: " admin ".to_string(),
            admin_password_hash: format!(" {VALID_BCRYPT_HASH} "),
        };

        let normalized = normalize_local_config(&config).unwrap();

        assert_eq!(
            normalized.database_url,
            "postgresql://union:secret@127.0.0.1:5432/union"
        );
        assert_eq!(normalized.admin_username, "admin");
        assert_eq!(normalized.admin_password_hash, VALID_BCRYPT_HASH);
    }

    #[test]
    fn rejects_non_postgresql_database_url() {
        let error = normalize_database_url("mysql://user:pass@localhost/db").unwrap_err();

        assert_eq!(error, LocalConfigError::UnsupportedDatabaseScheme);
        assert_eq!(error.code(), "local_config_database_url_unsupported_scheme");
    }

    #[test]
    fn hardens_group_readable_config_file() {
        let root =
            std::env::temp_dir().join(format!("union-local-config-test-{}", uuid::Uuid::new_v4()));
        let path = root.join("union-config.json");
        fs::create_dir_all(&root).unwrap();
        fs::write(&path, "{}").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();

        ensure_private_config_file(&path).unwrap();

        let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, LOCAL_CONFIG_FILE_MODE);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn rejects_symlinked_config_file() {
        let root =
            std::env::temp_dir().join(format!("union-local-config-test-{}", uuid::Uuid::new_v4()));
        let target = root.join("target.json");
        let link = root.join("union-config.json");
        fs::create_dir_all(&root).unwrap();
        fs::write(&target, "{}").unwrap();
        std::os::unix::fs::symlink(&target, &link).unwrap();

        assert!(ensure_private_config_file(&link).is_err());
        let _ = fs::remove_dir_all(root);
    }
}
