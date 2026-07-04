//! 应用启动编排。
//!
//! 将配置、数据库和运行时状态准备集中在这里，使二进制入口只负责日志、监听和关停。

use std::net::{IpAddr, SocketAddr};

use crate::{
    app_config::{LocalConfig, Settings, ensure_layout, load_local_config, save_local_config},
    blog, database, ram_auth, secrets, service_manager,
    state::AppState,
};

pub struct InitializedApp {
    pub addr: SocketAddr,
    pub state: AppState,
}

pub async fn initialize() -> anyhow::Result<InitializedApp> {
    // Settings::load() 此时只读取“启动所必需”的配置。数据库中的运行配置要等连接
    // 建立后才能读取，因此启动过程有 bootstrap_settings 和最终 settings 两个阶段。
    let bootstrap_settings = Settings::load()?;
    ensure_layout(&bootstrap_settings)?;
    secrets::init()?;

    let local_config = load_or_create_local_config(&bootstrap_settings).await?;
    // 允许第一次运行时没有数据库：用户仍能登录管理端，再通过设置页保存连接串。
    // 这种模式使用 disconnected_pool 占位，访问依赖数据库的 API 会被中间件拒绝。
    let database_configured = !bootstrap_settings.database.url.trim().is_empty();
    let (settings, db) = prepare_database(bootstrap_settings, database_configured).await?;
    let addr = listen_address(&settings)?;
    let dummy_password_hash = hash_password(uuid::Uuid::new_v4().to_string()).await?;
    let state = AppState::new(settings, db, dummy_password_hash, local_config);

    if database_configured {
        start_maintenance(state.db().as_ref().clone());
        initialize_database_backed_services(&state).await?;
    }

    Ok(InitializedApp { addr, state })
}

async fn load_or_create_local_config(settings: &Settings) -> anyhow::Result<LocalConfig> {
    match load_local_config() {
        Ok(config) => Ok(config),
        Err(err) if is_not_found(&err) => create_local_config(settings).await,
        Err(err) => Err(err),
    }
}

fn is_not_found(error: &anyhow::Error) -> bool {
    error
        .downcast_ref::<std::io::Error>()
        .is_some_and(|error| error.kind() == std::io::ErrorKind::NotFound)
}

async fn create_local_config(settings: &Settings) -> anyhow::Result<LocalConfig> {
    let configured_password = std::env::var("UNION_BOOTSTRAP_PASSWORD").ok();
    let password = bootstrap_password(configured_password.as_deref(), settings.production)?;
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
        tracing::warn!("开发环境首次启动密码已打印；生产环境不会采用此流程。");
    }
    Ok(config)
}

fn bootstrap_password(configured: Option<&str>, production: bool) -> anyhow::Result<String> {
    match configured {
        Some(password) if password.len() >= 12 => Ok(password.to_string()),
        Some(_) => anyhow::bail!("UNION_BOOTSTRAP_PASSWORD must be at least 12 characters"),
        None if production => anyhow::bail!(
            "UNION_BOOTSTRAP_PASSWORD is required when creating the first production administrator"
        ),
        None => Ok(uuid::Uuid::new_v4().to_string().replace('-', "")),
    }
}

async fn hash_password(password: String) -> anyhow::Result<String> {
    // bcrypt 是 CPU 密集型同步计算。spawn_blocking 把它移出 Tokio 异步工作线程，
    // 避免一次密码哈希阻塞同一线程上的其他 HTTP 请求。
    tokio::task::spawn_blocking(move || bcrypt::hash(password, bcrypt::DEFAULT_COST))
        .await
        .map_err(|error| anyhow::anyhow!("bcrypt task error: {error}"))?
        .map_err(|error| anyhow::anyhow!("bcrypt hash error: {error}"))
}

async fn prepare_database(
    bootstrap_settings: Settings,
    configured: bool,
) -> anyhow::Result<(Settings, database::DbPool)> {
    if !configured {
        tracing::warn!("数据库尚未配置；请登录控制台后在设置中配置 PostgreSQL");
        return Ok((bootstrap_settings, database::disconnected_pool()?));
    }

    let db = database::connect(&bootstrap_settings).await?;
    // 迁移必须早于任何业务查询，否则新数据库中还不存在 settings 等表。
    database::migrate(&db).await?;
    // 数据库里的配置是管理源；环境变量只覆盖明确规定可在运行时覆盖的字段。
    let mut settings = database::load_or_seed_app_settings(&db, &bootstrap_settings).await?;
    settings.apply_runtime_environment()?;
    ensure_layout(&settings)?;
    ram_auth::ensure_seeded(&db, &settings).await?;
    Ok((settings, db))
}

fn listen_address(settings: &Settings) -> anyhow::Result<SocketAddr> {
    // 分开解析 IP 与端口，避免 IPv6 直接拼接成 `:::8080` 这类无效地址。
    let bind_ip: IpAddr = settings
        .server
        .bind
        .trim()
        .trim_matches(['[', ']'])
        .parse()?;
    Ok(SocketAddr::new(bind_ip, settings.server.port))
}

fn start_maintenance(db: database::DbPool) {
    // clamp 防止配置错误导致刚产生的记录被清理，或历史数据永不清理。
    let retention_days = std::env::var("UNION_RETENTION_DAYS")
        .ok()
        .and_then(|value| value.parse::<i64>().ok())
        .unwrap_or(90)
        .clamp(7, 3650);
    tokio::spawn(maintenance_loop(db, retention_days));
}

async fn initialize_database_backed_services(state: &AppState) -> anyhow::Result<()> {
    // 孤立文章以草稿收养，防止 Astro 把未纳管内容直接发布。
    match blog::adopt_orphan_posts(state).await {
        Ok(0) => {}
        Ok(count) => tracing::info!("startup: adopted {count} orphan blog post(s) as drafts"),
        Err(error) => tracing::warn!("startup: failed to adopt orphan blog posts: {error}"),
    }

    if database::service_desired_state(state.db().as_ref(), "ram")
        .await?
        .as_deref()
        == Some("running")
        && let Err(error) = service_manager::start_ram(state).await
    {
        tracing::warn!("startup: failed to restore desired ram state: {error}");
    }
    Ok(())
}

async fn maintenance_loop(db: database::DbPool, retention_days: i64) {
    loop {
        match database::prune_operational_history(&db, retention_days).await {
            Ok(count) if count > 0 => {
                tracing::info!("maintenance: removed {count} old history rows")
            }
            Ok(_) => {}
            Err(error) => tracing::warn!("maintenance: failed to prune history: {error}"),
        }
        tokio::time::sleep(std::time::Duration::from_secs(24 * 60 * 60)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_configured_bootstrap_password() {
        assert!(bootstrap_password(Some("short"), false).is_err());
        assert_eq!(
            bootstrap_password(Some("long-enough-password"), true).unwrap(),
            "long-enough-password"
        );
    }

    #[test]
    fn production_requires_configured_bootstrap_password() {
        assert!(bootstrap_password(None, true).is_err());
        assert_eq!(bootstrap_password(None, false).unwrap().len(), 32);
    }
}
