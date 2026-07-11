//! Axum 路由共享状态。

use std::{collections::HashMap, sync::Arc, time::Instant};

use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};
use tokio::sync::{Mutex, RwLock};

use crate::{
    app_config::{LocalConfig, Settings, SunshineHostConfig},
    database::DbPool,
};

#[derive(Clone)]
pub struct AppState {
    pub settings: Arc<Settings>,
    database: Arc<DbPool>,
    pub database_health: Arc<Mutex<Option<DatabaseHealthSnapshot>>>,
    pub started_at: DateTime<Utc>,
    pub hosts: HostState,
    pub auth: AuthenticationState,
    pub agents: AgentAuthenticationState,
}

#[derive(Clone)]
pub struct AgentAuthenticationState {
    enrollment_token_hash: Option<[u8; 32]>,
    pub registration_attempts: Arc<Mutex<Vec<Instant>>>,
}

#[derive(Clone)]
pub struct HostState {
    pub sunshine: Arc<RwLock<Vec<SunshineHostConfig>>>,
    pub settings_lock: Arc<Mutex<()>>,
}

#[derive(Clone)]
pub struct AuthenticationState {
    pub sse_tickets: Arc<Mutex<HashMap<String, Instant>>>,
    pub login_attempts: Arc<Mutex<LoginAttemptState>>,
    pub bcrypt_limit: Arc<tokio::sync::Semaphore>,
    pub dummy_password_hash: Arc<String>,
    pub local_config: Arc<RwLock<LocalConfig>>,
    pub sessions: Arc<RwLock<HashMap<String, LocalSession>>>,
}

#[derive(Debug, Clone)]
pub struct LocalSession {
    pub username: String,
    pub expires_at: DateTime<Utc>,
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

impl AppState {
    pub fn new(
        settings: Settings,
        db: DbPool,
        dummy_password_hash: String,
        local_config: LocalConfig,
    ) -> Self {
        let sunshine_hosts = settings.sunshine.hosts.clone();
        let enrollment_token_hash = (!settings.agents.enrollment_token.is_empty())
            .then(|| Sha256::digest(settings.agents.enrollment_token.as_bytes()).into());
        Self {
            settings: Arc::new(settings),
            database: Arc::new(db),
            database_health: Arc::new(Mutex::new(None)),
            started_at: Utc::now(),
            hosts: HostState {
                sunshine: Arc::new(RwLock::new(sunshine_hosts)),
                settings_lock: Arc::new(Mutex::new(())),
            },
            auth: AuthenticationState {
                sse_tickets: Arc::new(Mutex::new(HashMap::new())),
                login_attempts: Arc::new(Mutex::new(LoginAttemptState::default())),
                bcrypt_limit: Arc::new(tokio::sync::Semaphore::new(4)),
                dummy_password_hash: Arc::new(dummy_password_hash),
                local_config: Arc::new(RwLock::new(local_config)),
                sessions: Arc::new(RwLock::new(HashMap::new())),
            },
            agents: AgentAuthenticationState {
                enrollment_token_hash,
                registration_attempts: Arc::new(Mutex::new(Vec::new())),
            },
        }
    }

    pub fn db(&self) -> Arc<DbPool> {
        self.database.clone()
    }

    pub fn agent_enrollment_configured(&self) -> bool {
        self.agents.enrollment_token_hash.is_some()
    }

    pub fn matches_agent_enrollment_token(&self, candidate: &str) -> bool {
        let Some(expected) = self.agents.enrollment_token_hash else {
            return false;
        };
        let actual: [u8; 32] = Sha256::digest(candidate.as_bytes()).into();
        expected
            .iter()
            .zip(actual)
            .fold(0_u8, |difference, (left, right)| {
                difference | (left ^ right)
            })
            == 0
    }
}
