//! UnionC 应用启动编排。

use std::net::{IpAddr, SocketAddr};

use crate::{
    app_config::{LocalConfig, Settings, ensure_layout, load_local_config, save_local_config},
    database, secrets,
    state::AppState,
};

pub struct InitializedApp {
    pub addr: SocketAddr,
    pub state: AppState,
}

pub async fn initialize() -> anyhow::Result<InitializedApp> {
    let bootstrap_settings = Settings::load()?;
    ensure_layout(&bootstrap_settings)?;
    secrets::init()?;
    let local_config = load_or_create_local_config(&bootstrap_settings).await?;
    let database_configured = !bootstrap_settings.database.url.trim().is_empty();
    let (settings, db) = prepare_database(bootstrap_settings, database_configured).await?;
    let addr = listen_address(&settings)?;
    let dummy_password_hash = hash_password(uuid::Uuid::new_v4().to_string()).await?;
    let state = AppState::new(settings, db, dummy_password_hash, local_config);
    if database_configured {
        start_maintenance(state.clone());
    }
    Ok(InitializedApp { addr, state })
}

async fn load_or_create_local_config(settings: &Settings) -> anyhow::Result<LocalConfig> {
    match load_local_config() {
        Ok(config) => Ok(config),
        Err(error)
            if error
                .downcast_ref::<std::io::Error>()
                .is_some_and(|error| error.kind() == std::io::ErrorKind::NotFound) =>
        {
            create_local_config(settings).await
        }
        Err(error) => Err(error),
    }
}

async fn create_local_config(settings: &Settings) -> anyhow::Result<LocalConfig> {
    let configured_password = std::env::var("UNIONC_BOOTSTRAP_PASSWORD").ok();
    let password = match configured_password.as_deref() {
        Some(password) if password.len() >= 12 => password.to_string(),
        Some(_) => anyhow::bail!("UNIONC_BOOTSTRAP_PASSWORD must be at least 12 characters"),
        None if settings.production => {
            anyhow::bail!("UNIONC_BOOTSTRAP_PASSWORD is required in production")
        }
        None => uuid::Uuid::new_v4().to_string().replace('-', ""),
    };
    let config = LocalConfig {
        database_url: settings.database.url.clone(),
        admin_username: "admin".to_string(),
        admin_password_hash: hash_password(password.clone()).await?,
    };
    save_local_config(&config)?;
    if configured_password.is_some() {
        tracing::warn!("首次启动：管理员账号已由部署提供的初始密码创建，请立即修改密码。");
    } else {
        eprintln!("首次启动管理员：admin / {password}");
    }
    Ok(config)
}

async fn hash_password(password: String) -> anyhow::Result<String> {
    tokio::task::spawn_blocking(move || bcrypt::hash(password, bcrypt::DEFAULT_COST))
        .await
        .map_err(|error| anyhow::anyhow!("bcrypt task error: {error}"))?
        .map_err(|error| anyhow::anyhow!("bcrypt hash error: {error}"))
}

async fn prepare_database(
    bootstrap: Settings,
    configured: bool,
) -> anyhow::Result<(Settings, database::DbPool)> {
    if !configured {
        tracing::warn!("数据库尚未配置；请登录 UnionC 后在设置中配置 PostgreSQL");
        return Ok((bootstrap, database::disconnected_pool()?));
    }
    let db = database::connect(&bootstrap).await?;
    database::migrate(&db).await?;
    let mut settings = database::load_or_seed_app_settings(&db, &bootstrap).await?;
    settings.apply_runtime_environment()?;
    ensure_layout(&settings)?;
    Ok((settings, db))
}

fn listen_address(settings: &Settings) -> anyhow::Result<SocketAddr> {
    let bind_ip: IpAddr = settings
        .server
        .bind
        .trim()
        .trim_matches(['[', ']'])
        .parse()?;
    Ok(SocketAddr::new(bind_ip, settings.server.port))
}

fn start_maintenance(state: AppState) {
    let retention_days = std::env::var("UNIONC_RETENTION_DAYS")
        .ok()
        .and_then(|value| value.parse::<i64>().ok())
        .unwrap_or(90)
        .clamp(7, 3650);
    let monitoring_retention_days = std::env::var("UNIONC_TELEMETRY_RETENTION_DAYS")
        .ok()
        .and_then(|value| value.parse::<i64>().ok())
        .unwrap_or(30)
        .clamp(1, 3650);
    tokio::spawn(async move {
        loop {
            match database::prune_audit_history(state.db().as_ref(), retention_days).await {
                Ok(removed) if removed > 0 => {
                    tracing::info!("maintenance: removed {removed} old audit rows")
                }
                Ok(_) => {}
                Err(error) => tracing::warn!("maintenance: failed to prune audit history: {error}"),
            }
            match database::prune_monitoring_history(state.db().as_ref(), monitoring_retention_days)
                .await
            {
                Ok(removed) if removed > 0 => {
                    tracing::info!("maintenance: removed {removed} old monitoring reports")
                }
                Ok(_) => {}
                Err(error) => {
                    tracing::warn!("maintenance: failed to prune monitoring history: {error}")
                }
            }
            tokio::time::sleep(std::time::Duration::from_secs(24 * 60 * 60)).await;
        }
    });
}
