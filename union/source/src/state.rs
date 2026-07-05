//! Axum 路由共享状态。
//!
//! 每个 HTTP 请求都会拿到一份 `AppState` 克隆。这里的克隆很轻量，因为大对象都放在
//! `Arc` 或连接池里共享，不会复制完整配置或进程对象。

use std::{collections::HashMap, sync::Arc, time::Instant};

use chrono::{DateTime, Utc};
use tokio::{
    process::Child,
    sync::{Mutex, RwLock},
};

use crate::{
    app_config::{LocalConfig, ProxmoxHostConfig, Settings, SunshineHostConfig},
    database::DbPool,
};

/// 整个后端共享的运行状态。
#[derive(Clone)]
pub struct AppState {
    /// 配置使用 Arc 共享给所有路由，避免每个请求复制完整配置。
    pub settings: Arc<Settings>,
    /// PostgreSQL 连接池，本身就是可克隆的轻量句柄。
    database: Arc<DbPool>,
    /// 业务路由数据库可用性短缓存，避免高频轮询每次都做 PostgreSQL 往返。
    pub database_health: Arc<Mutex<Option<DatabaseHealthSnapshot>>>,
    /// 当前由union托管的 ram 子进程。
    pub ram: Arc<ProcessSlot>,
    /// 后端启动时间，用于计算 uptime。
    pub started_at: DateTime<Utc>,
    /// 可变外部主机注册表及其持久化事务锁。
    pub hosts: HostState,
    /// 博客构建与内容写入协调状态。
    pub blog: BlogState,
    /// 登录、会话和 SSE 认证状态。
    pub auth: AuthenticationState,
}

#[derive(Clone)]
pub struct HostState {
    pub proxmox: Arc<RwLock<Vec<ProxmoxHostConfig>>>,
    pub sunshine: Arc<RwLock<Vec<SunshineHostConfig>>>,
    /// 串行化两类主机配置的读改写，防止相互覆盖。
    pub settings_lock: Arc<Mutex<()>>,
}

#[derive(Clone)]
pub struct BlogState {
    pub build: Arc<Mutex<BlogBuildState>>,
    /// 串行化数据库与导出文件的复合写入。
    pub content_lock: Arc<Mutex<()>>,
}

#[derive(Clone)]
pub struct AuthenticationState {
    /// SSE 一次性票据与签发时间。
    pub sse_tickets: Arc<Mutex<HashMap<String, Instant>>>,
    pub login_attempts: Arc<Mutex<LoginAttemptState>>,
    pub bcrypt_limit: Arc<tokio::sync::Semaphore>,
    pub dummy_password_hash: Arc<String>,
    pub local_config: Arc<RwLock<LocalConfig>>,
    /// 本机内存会话，重启后自动失效。
    pub sessions: Arc<RwLock<HashMap<String, LocalSession>>>,
}

#[derive(Debug, Clone)]
pub struct LocalSession {
    pub username: String,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Default)]
pub struct BlogBuildState {
    pub running: bool,
    pub dirty: bool,
}

#[derive(Debug, Default)]
pub struct LoginAttemptState {
    pub global: Vec<Instant>,
    pub by_username: HashMap<String, Vec<Instant>>,
}

#[derive(Debug, Clone)]
pub struct DatabaseHealthSnapshot {
    pub checked_at: Instant,
    pub available: bool,
}

/// 可变进程槽位。
pub struct ProcessSlot {
    /// 串行化完整启停流程；child 锁只保护句柄本身，不能覆盖 spawn 前的异步窗口。
    pub operation: Mutex<()>,
    /// Mutex 保护子进程句柄，确保启动、停止、状态检查不会并发修改同一个进程。
    pub child: Mutex<Option<ManagedProcess>>,
}

/// 被union托管的进程信息。
pub struct ManagedProcess {
    /// Tokio 子进程句柄，可用于等待或杀掉进程。
    pub child: Child,
    /// 操作系统进程号。
    pub pid: Option<u32>,
    /// 进程启动时间。
    pub started_at: DateTime<Utc>,
}

impl AppState {
    /// 创建共享状态。调用方需要先完成配置加载和数据库连接。
    pub fn new(
        settings: Settings,
        db: DbPool,
        dummy_password_hash: String,
        local_config: LocalConfig,
    ) -> Self {
        let sunshine_hosts = settings.sunshine.hosts.clone();
        let proxmox_hosts = settings.proxmox.hosts.clone();
        Self {
            settings: Arc::new(settings),
            database: Arc::new(db),
            database_health: Arc::new(Mutex::new(None)),
            ram: Arc::new(ProcessSlot {
                operation: Mutex::new(()),
                child: Mutex::new(None),
            }),
            started_at: Utc::now(),
            hosts: HostState {
                proxmox: Arc::new(RwLock::new(proxmox_hosts)),
                sunshine: Arc::new(RwLock::new(sunshine_hosts)),
                settings_lock: Arc::new(Mutex::new(())),
            },
            blog: BlogState {
                build: Arc::new(Mutex::new(BlogBuildState::default())),
                content_lock: Arc::new(Mutex::new(())),
            },
            auth: AuthenticationState {
                sse_tickets: Arc::new(Mutex::new(HashMap::new())),
                login_attempts: Arc::new(Mutex::new(LoginAttemptState::default())),
                bcrypt_limit: Arc::new(tokio::sync::Semaphore::new(4)),
                dummy_password_hash: Arc::new(dummy_password_hash),
                local_config: Arc::new(RwLock::new(local_config)),
                sessions: Arc::new(RwLock::new(HashMap::new())),
            },
        }
    }

    /// 获取启动时装载的数据库池。设置页只验证并保存新连接串，完整切换在重启后生效。
    pub fn db(&self) -> Arc<DbPool> {
        self.database.clone()
    }
}
