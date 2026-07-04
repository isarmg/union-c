//! 启动环境覆盖、本地私有配置和目录初始化。

use super::*;

impl Settings {
    /// 加载配置。
    ///
    /// 这里只允许读取 PostgreSQL 启动连接串；其它运行配置统一从 PostgreSQL 的 `settings` 表读取。
    pub fn load() -> anyhow::Result<Self> {
        let mut settings = Settings::default();
        if let Ok(config) = load_local_config() {
            settings.database.url = config.database_url;
        }
        settings.apply_runtime_environment()?;
        Ok(settings)
    }

    /// 把只能由部署环境决定的安全配置应用到数据库配置之上。
    pub fn apply_runtime_environment(&mut self) -> anyhow::Result<()> {
        self.production =
            env::var("UNION_ENV").is_ok_and(|value| value.eq_ignore_ascii_case("production"));

        if let Ok(url) = env::var("UNION_DATABASE_URL").or_else(|_| env::var("DATABASE_URL")) {
            self.database.url = url;
        }
        self.ram.public_url = env::var("UNION_RAM_PUBLIC_URL")
            .ok()
            .map(|value| value.trim_end_matches('/').to_string())
            .filter(|value| !value.is_empty());
        if self.production {
            let bind: IpAddr = self
                .server
                .bind
                .trim()
                .trim_matches(['[', ']'])
                .parse()
                .map_err(|_| anyhow::anyhow!("invalid server bind address"))?;
            if !bind.is_loopback() {
                anyhow::bail!(
                    "production union must bind to a loopback address behind the reverse proxy"
                );
            }

            let ram_bind: IpAddr = self
                .ram
                .bind
                .trim()
                .trim_matches(['[', ']'])
                .parse()
                .map_err(|_| anyhow::anyhow!("invalid ram bind address"))?;
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
            if self
                .ram
                .auth
                .iter()
                .any(|rule| rule.contains("change-me") || rule.contains("guest:guest"))
                || self
                    .ram
                    .management_auth
                    .as_deref()
                    .is_some_and(|value| value.contains("change-me"))
            {
                anyhow::bail!("production refuses known default ram credentials");
            }
        }
        Ok(())
    }
}

pub fn load_local_config() -> anyhow::Result<LocalConfig> {
    let content = fs::read_to_string(LOCAL_CONFIG_PATH)?;
    Ok(serde_json::from_str(&content)?)
}

pub fn save_local_config(config: &LocalConfig) -> anyhow::Result<()> {
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    fs::create_dir_all("data")?;
    let temporary = format!("{LOCAL_CONFIG_PATH}.tmp");
    let mut file = fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .mode(0o600)
        .open(&temporary)?;
    serde_json::to_writer_pretty(&mut file, config)?;
    use std::io::Write;
    file.write_all(b"\n")?;
    file.sync_all()?;
    fs::set_permissions(&temporary, fs::Permissions::from_mode(0o600))?;
    fs::rename(temporary, LOCAL_CONFIG_PATH)?;
    fs::File::open("data")?.sync_all()?;
    Ok(())
}

/// 确保项目运行需要的目录全部存在。
///
/// `create_dir_all` 类似命令行里的 `mkdir -p`：目录已经存在不会报错，父目录不存在会一起创建。
pub fn ensure_layout(settings: &Settings) -> std::io::Result<()> {
    if settings.production {
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
    }
    // 首次启动时补齐目录结构，避免后续写日志、写文章或启动 ram 时才失败。
    let dirs = [
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
    ];

    for dir in dirs {
        fs::create_dir_all(dir)?;
    }

    if let Some(parent) = settings.ram.log_path.parent() {
        fs::create_dir_all(parent)?;
    }
    if let Some(parent) = settings.ram.process_log_path.parent() {
        fs::create_dir_all(parent)?;
    }
    for host in &settings.sunshine.hosts {
        if let Some(parent) = host.log_path.parent() {
            fs::create_dir_all(parent)?;
        }
    }
    fs::create_dir_all(&settings.blog.build_log_dir)?;

    // 运行数据不应被同机其他账号遍历。构建产物不在 data/ 下，不受此限制。
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions("data", fs::Permissions::from_mode(0o700))?;

    Ok(())
}
