//! API 访问控制中间件。
//!
//! 这里集中处理会话认证、SSE 短效票据、数据库可用性检查和 Cookie CSRF 防护。
//! 路由模块只负责请求解析和业务调用，不重复实现安全策略。

use std::time::{Duration, Instant};

use axum::{
    extract::{Request, State},
    http::Method,
    middleware::Next,
    response::Response,
};

use crate::{database, error::AppError, state::AppState};

use super::auth;

const DATABASE_HEALTH_TTL: Duration = Duration::from_secs(1);

pub(super) async fn require_auth(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Result<Response, AppError> {
    let path = request.uri().path();

    if is_public_path(path) {
        return Ok(next.run(request).await);
    }

    // EventSource 不支持自定义请求头，因此 SSE 使用一次性短效票据。
    if path == "/api/events"
        && let Some(ticket) = sse_ticket(request.uri().query()).map(str::to_owned)
    {
        return authenticate_sse(&state, &ticket, request, next).await;
    }

    let token = auth::session_cookie(request.headers()).ok_or(AppError::Unauthorized)?;
    let username = auth::local_session_user(&state, &token).await?;

    ensure_database_available(&state, path).await?;
    ensure_csrf_protected(&request, true)?;

    let request_id = request
        .headers()
        .get("x-request-id")
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    Ok(database::with_audit_context(
        database::AuditContext {
            actor: username,
            request_id,
        },
        next.run(request),
    )
    .await)
}

fn is_public_path(path: &str) -> bool {
    matches!(
        path,
        "/api/health" | "/api/ready" | "/api/auth/login" | "/api/auth/basic"
    )
}

fn sse_ticket(query: Option<&str>) -> Option<&str> {
    query?
        .split('&')
        .find_map(|parameter| parameter.strip_prefix("ticket="))
}

async fn authenticate_sse(
    state: &AppState,
    ticket: &str,
    request: Request,
    next: Next,
) -> Result<Response, AppError> {
    let issued_at = state.auth.sse_tickets.lock().await.remove(ticket);
    if issued_at.is_none_or(|issued_at| issued_at.elapsed() >= Duration::from_secs(60)) {
        return Err(AppError::Unauthorized);
    }

    ensure_database_available(state, "/api/events").await?;
    Ok(next.run(request).await)
}

async fn ensure_database_available(state: &AppState, path: &str) -> Result<(), AppError> {
    if requires_database(path) && !database_available(state).await {
        return Err(AppError::DatabaseUnavailable(
            "数据库未连接，请先在设置中配置数据库".to_string(),
        ));
    }
    Ok(())
}

async fn database_available(state: &AppState) -> bool {
    let now = Instant::now();
    {
        let cached = state.database_health.lock().await;
        if let Some(snapshot) = cached.as_ref()
            && now.duration_since(snapshot.checked_at) < DATABASE_HEALTH_TTL
        {
            return snapshot.available;
        }
    }

    let available = database::ping(state.db().as_ref()).await.is_ok();
    *state.database_health.lock().await = Some(crate::state::DatabaseHealthSnapshot {
        checked_at: Instant::now(),
        available,
    });
    available
}

fn ensure_csrf_protected(request: &Request, cookie_authenticated: bool) -> Result<(), AppError> {
    if cookie_authenticated
        && is_state_changing(request.method())
        && request
            .headers()
            .get("x-csrf-token")
            .and_then(|value| value.to_str().ok())
            != Some("1")
    {
        return Err(AppError::Forbidden(
            "missing or invalid CSRF protection header".to_string(),
        ));
    }
    Ok(())
}

fn is_state_changing(method: &Method) -> bool {
    !matches!(*method, Method::GET | Method::HEAD | Method::OPTIONS)
}

fn requires_database(path: &str) -> bool {
    path == "/api/events"
        || path == "/api/events/ticket"
        || path.starts_with("/api/services")
        || path.starts_with("/api/blog")
        || path.starts_with("/api/pve")
}
