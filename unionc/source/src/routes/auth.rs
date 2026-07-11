//! 认证与账号管理 handler。
//!
//! # 认证流程
//!
//! 管理台使用 JSON 登录，验证成功后通过 HttpOnly Cookie 建立会话。
//!
//! Token 是随机 UUID，仅保存在进程内存；有效期 7 天，重启或改密后失效。

use axum::{
    Json, Router,
    extract::State,
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use base64::{Engine as _, engine::general_purpose::STANDARD};

use crate::{
    app_config::save_local_config,
    domain::{ChangePasswordRequest, LoginRequest, LoginResponse, UserInfoResponse},
    error::{AppError, AppResult},
    state::{AppState, LocalSession},
};

const LOGIN_WINDOW: std::time::Duration = std::time::Duration::from_secs(60);
const MAX_LOGIN_ATTEMPTS: usize = 5;
const MAX_GLOBAL_LOGIN_ATTEMPTS: usize = 60;

pub(super) fn router() -> Router<AppState> {
    Router::new()
        .route("/api/auth/login", post(login))
        .route("/api/auth/basic", get(basic_login))
        .route("/api/auth/logout", post(logout))
        .route("/api/auth/me", get(me))
        .route("/api/auth/change-password", post(change_password))
}

/// 从 Cookie 头提取 session cookie 的值。
pub(super) fn session_cookie(headers: &HeaderMap) -> Option<String> {
    let cookie_str = headers.get(header::COOKIE).and_then(|v| v.to_str().ok())?;
    let mut legacy = None;
    for part in cookie_str.split(';') {
        let part = part.trim();
        if let Some(value) = part.strip_prefix("__Host-session=") {
            return Some(value.to_string());
        }
        if legacy.is_none()
            && let Some(value) = part.strip_prefix("session=")
        {
            legacy = Some(value.to_string());
        }
    }
    legacy
}

/// 管理台只使用 HttpOnly Cookie，不把长效会话令牌暴露给 JavaScript。
pub(super) fn extract_token(headers: &HeaderMap) -> Option<String> {
    session_cookie(headers)
}

async fn authenticate(state: &AppState, username: &str, password: String) -> AppResult<String> {
    let username = username.trim();
    let key = username.to_ascii_lowercase();
    let now = std::time::Instant::now();
    {
        let mut attempts = state.auth.login_attempts.lock().await;
        attempts
            .global
            .retain(|instant| now.duration_since(*instant) < LOGIN_WINDOW);
        attempts.by_username.retain(|_, values| {
            values.retain(|instant| now.duration_since(*instant) < LOGIN_WINDOW);
            !values.is_empty()
        });
        if attempts.global.len() >= MAX_GLOBAL_LOGIN_ATTEMPTS
            || attempts
                .by_username
                .get(&key)
                .is_some_and(|values| values.len() >= MAX_LOGIN_ATTEMPTS)
        {
            return Err(AppError::TooManyRequests(
                "登录尝试过于频繁，请一分钟后再试".to_string(),
            ));
        }
        // 在昂贵校验前占用名额，避免并发请求同时穿过限流检查。
        attempts.global.push(now);
        attempts
            .by_username
            .entry(key.clone())
            .or_default()
            .push(now);
    }

    let permit = bcrypt_permit(state)?;
    let config = state.auth.local_config.read().await;
    let known_user = username == config.admin_username;
    let hash = if known_user {
        config.admin_password_hash.clone()
    } else {
        (*state.auth.dummy_password_hash).clone()
    };
    let configured_username = config.admin_username.clone();
    drop(config);
    let valid = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        bcrypt::verify(&password, &hash)
    })
    .await
    .map_err(|e| AppError::Anyhow(anyhow::anyhow!("bcrypt task error: {e}")))?
    .map_err(|e| AppError::Anyhow(anyhow::anyhow!("bcrypt verify error: {e}")))?;

    match (valid, known_user) {
        (true, true) => {
            state
                .auth
                .login_attempts
                .lock()
                .await
                .by_username
                .remove(&key);
            Ok(configured_username)
        }
        _ => Err(AppError::Unauthorized),
    }
}

fn bcrypt_permit(state: &AppState) -> AppResult<tokio::sync::OwnedSemaphorePermit> {
    state
        .auth
        .bcrypt_limit
        .clone()
        .try_acquire_owned()
        .map_err(|_| AppError::TooManyRequests("密码校验繁忙，请稍后再试".to_string()))
}

// ─── Handlers ─────────────────────────────────────────────────────────────────

/// POST /api/auth/login — JSON 登录，同时返回程序令牌和浏览器 Cookie。
pub(super) async fn login(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<LoginRequest>,
) -> AppResult<Response> {
    require_https_in_production(&state, &headers)?;
    let user = authenticate(&state, &payload.username, payload.password).await?;
    create_login_response(&state, user).await
}

/// GET /api/auth/basic — 触发浏览器原生 HTTP Basic 登录框并换取应用会话。
pub(super) async fn basic_login(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> AppResult<Response> {
    require_https_in_production(&state, &headers)?;
    let Some((username, password)) = basic_credentials(&headers) else {
        return Ok(basic_auth_challenge());
    };
    match authenticate(&state, &username, password).await {
        Ok(user) => create_login_response(&state, user).await,
        Err(AppError::Unauthorized) => Ok(basic_auth_challenge()),
        Err(error) => Err(error),
    }
}

fn basic_credentials(headers: &HeaderMap) -> Option<(String, String)> {
    let encoded = headers
        .get(header::AUTHORIZATION)?
        .to_str()
        .ok()?
        .strip_prefix("Basic ")?;
    let decoded = String::from_utf8(STANDARD.decode(encoded).ok()?).ok()?;
    let (username, password) = decoded.split_once(':')?;
    Some((username.to_string(), password.to_string()))
}

fn basic_auth_challenge() -> Response {
    let mut response = StatusCode::UNAUTHORIZED.into_response();
    response.headers_mut().insert(
        header::WWW_AUTHENTICATE,
        HeaderValue::from_static("Basic realm=\"UnionC\", charset=\"UTF-8\""),
    );
    response
}

async fn create_login_response(state: &AppState, username: String) -> AppResult<Response> {
    let token = uuid::Uuid::new_v4().to_string();
    let expires_at = chrono::Utc::now() + chrono::Duration::days(7);
    let mut sessions = state.auth.sessions.write().await;
    sessions.retain(|_, session| session.expires_at > chrono::Utc::now());
    sessions.insert(
        token.clone(),
        LocalSession {
            username: username.clone(),
            expires_at,
        },
    );
    drop(sessions);

    let cookie = session_cookie_value(&token, state.settings.production, 604800);
    let mut response = Json(LoginResponse { username }).into_response();
    response
        .headers_mut()
        .insert(header::SET_COOKIE, cookie_header(&cookie)?);
    Ok(response)
}

fn require_https_in_production(state: &AppState, headers: &HeaderMap) -> AppResult<()> {
    if state.settings.production
        && headers
            .get("x-forwarded-proto")
            .and_then(|value| value.to_str().ok())
            != Some("https")
    {
        return Err(AppError::Forbidden(
            "production login is only available through the HTTPS reverse proxy".to_string(),
        ));
    }
    Ok(())
}

/// POST /api/auth/logout — 删除会话，清除 cookie。
pub(super) async fn logout(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> AppResult<Response> {
    if let Some(token) = extract_token(&headers) {
        state.auth.sessions.write().await.remove(&token);
    }
    let mut response = StatusCode::NO_CONTENT.into_response();
    response.headers_mut().insert(
        header::SET_COOKIE,
        cookie_header(&session_cookie_value("", state.settings.production, 0))?,
    );
    Ok(response)
}

fn session_cookie_value(token: &str, secure: bool, max_age: u64) -> String {
    format!(
        "{}={token}; Path=/; HttpOnly; SameSite=Strict; Max-Age={max_age}{}",
        if secure { "__Host-session" } else { "session" },
        if secure { "; Secure" } else { "" }
    )
}

fn cookie_header(cookie: &str) -> AppResult<HeaderValue> {
    HeaderValue::from_str(cookie)
        .map_err(|error| AppError::Anyhow(anyhow::anyhow!("invalid session cookie: {error}")))
}

/// GET /api/auth/me — 返回当前登录用户名。
pub(super) async fn me(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> AppResult<Json<UserInfoResponse>> {
    let token = extract_token(&headers).ok_or(AppError::Unauthorized)?;
    let user = local_session_user(&state, &token).await?;
    Ok(Json(UserInfoResponse { username: user }))
}

/// POST /api/auth/change-password — 修改密码，使其他设备会话失效。
pub(super) async fn change_password(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<ChangePasswordRequest>,
) -> AppResult<StatusCode> {
    let token = extract_token(&headers).ok_or(AppError::Unauthorized)?;
    let username = local_session_user(&state, &token).await?;
    let current_hash = state
        .auth
        .local_config
        .read()
        .await
        .admin_password_hash
        .clone();

    let hash = current_hash;
    let current_pw = payload.current_password.clone();
    let permit = bcrypt_permit(&state)?;
    let valid = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        bcrypt::verify(&current_pw, &hash)
    })
    .await
    .map_err(|e| AppError::Anyhow(anyhow::anyhow!("bcrypt task error: {e}")))?
    .map_err(|e| AppError::Anyhow(anyhow::anyhow!("bcrypt verify error: {e}")))?;

    if !valid {
        return Err(AppError::BadRequest("当前密码不正确".to_string()));
    }
    if payload.new_password.len() < 12 {
        return Err(AppError::BadRequest("新密码至少需要 12 个字符".to_string()));
    }

    let new_pw = payload.new_password.clone();
    let permit = bcrypt_permit(&state)?;
    let new_hash = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        bcrypt::hash(&new_pw, bcrypt::DEFAULT_COST)
    })
    .await
    .map_err(|e| AppError::Anyhow(anyhow::anyhow!("bcrypt task error: {e}")))?
    .map_err(|e| AppError::Anyhow(anyhow::anyhow!("bcrypt hash error: {e}")))?;

    {
        let mut config = state.auth.local_config.write().await;
        config.admin_password_hash = new_hash;
        save_local_config(&config).map_err(AppError::Anyhow)?;
    }
    state
        .auth
        .sessions
        .write()
        .await
        .retain(|session_token, session| session_token == &token || session.username != username);

    Ok(StatusCode::NO_CONTENT)
}

pub(super) async fn local_session_user(state: &AppState, token: &str) -> AppResult<String> {
    let mut sessions = state.auth.sessions.write().await;
    sessions.retain(|_, session| session.expires_at > chrono::Utc::now());
    sessions
        .get(token)
        .map(|session| session.username.clone())
        .ok_or(AppError::Unauthorized)
}
