//! Read-only multi-host monitoring endpoints.

use std::time::{Duration, Instant};

use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Path, Query, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use chrono::Utc;
use sha2::{Digest, Sha256};

use crate::{
    database,
    domain::{
        AgentRegistrationRequest, AgentRegistrationResponse, AgentReport, AgentReportResponse,
        HistoryPoint, HistoryQuery, HistoryResponse, HostDetailResponse, HostListResponse,
        HostSummary, MetricSummary,
    },
    error::{AppError, AppResult},
    state::AppState,
};

const MAX_REGISTRATIONS_PER_MINUTE: usize = 60;

pub(super) fn console_router() -> Router<AppState> {
    Router::new()
        .route("/api/monitoring/hosts", get(list_hosts))
        .route("/api/monitoring/hosts/{host_id}", get(host_detail))
        .route("/api/monitoring/hosts/{host_id}/history", get(host_history))
}

pub(super) fn agent_router() -> Router<AppState> {
    Router::new()
        .route("/api/agent/v1/register", post(register_agent))
        .route("/api/agent/v1/report", post(report_metrics))
        .layer(DefaultBodyLimit::max(512 * 1024))
}

async fn register_agent(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<AgentRegistrationRequest>,
) -> AppResult<Response> {
    require_agent_https(&state, &headers)?;
    if !state.agent_enrollment_configured() {
        return Err(AppError::ServiceUnavailable(
            "agent enrollment is not configured".to_string(),
        ));
    }
    let credential = bearer_token(&headers).ok_or(AppError::Unauthorized)?;
    if !state.matches_agent_enrollment_token(credential) {
        return Err(AppError::Unauthorized);
    }
    check_registration_rate(&state).await?;
    request.validate()?;
    require_database(&state).await?;

    let token = new_agent_token();
    let registered = database::register_monitoring_host(
        state.db().as_ref(),
        &request.host,
        &token_hash(&request.enrollment_secret),
        &token_hash(&token),
    )
    .await
    .map_err(AppError::Anyhow)?;
    if !registered {
        return Err(AppError::Conflict(
            "host id is already registered with a different enrollment secret".to_string(),
        ));
    }
    let mut response = (
        StatusCode::CREATED,
        Json(AgentRegistrationResponse {
            host_id: uuid::Uuid::parse_str(&request.host.id)
                .expect("validated host UUID")
                .to_string(),
            token,
        }),
    )
        .into_response();
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    Ok(response)
}

async fn report_metrics(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(report): Json<AgentReport>,
) -> AppResult<Response> {
    require_agent_https(&state, &headers)?;
    let credential = bearer_token(&headers).ok_or(AppError::Unauthorized)?;
    require_database(&state).await?;
    let authenticated_host =
        database::monitoring_host_for_token(state.db().as_ref(), &token_hash(credential))
            .await
            .map_err(AppError::Anyhow)?
            .ok_or(AppError::Unauthorized)?;
    report.validate()?;
    let reported_host = uuid::Uuid::parse_str(&report.host.id)
        .expect("validated host UUID")
        .to_string();
    if reported_host != authenticated_host {
        return Err(AppError::Forbidden(
            "agent token does not belong to the reported host".to_string(),
        ));
    }
    let (accepted, received_at) = database::store_monitoring_report(state.db().as_ref(), &report)
        .await
        .map_err(AppError::Anyhow)?;
    Ok((
        StatusCode::ACCEPTED,
        Json(AgentReportResponse {
            host_id: authenticated_host,
            report_id: report.report_id,
            accepted,
            received_at,
        }),
    )
        .into_response())
}

async fn list_hosts(State(state): State<AppState>) -> AppResult<Json<HostListResponse>> {
    let hosts = database::list_monitored_hosts(state.db().as_ref())
        .await
        .map_err(AppError::Anyhow)?
        .into_iter()
        .map(host_summary)
        .collect();
    Ok(Json(HostListResponse { hosts }))
}

async fn host_detail(
    State(state): State<AppState>,
    Path(host_id): Path<String>,
) -> AppResult<Json<HostDetailResponse>> {
    validate_host_id(&host_id)?;
    let stored = database::get_monitored_host(state.db().as_ref(), &host_id)
        .await
        .map_err(AppError::Anyhow)?
        .ok_or_else(|| AppError::NotFound("monitored host not found".to_string()))?;
    let latest = stored.latest.clone();
    Ok(Json(HostDetailResponse {
        host: host_summary(stored),
        latest,
    }))
}

async fn host_history(
    State(state): State<AppState>,
    Path(host_id): Path<String>,
    Query(query): Query<HistoryQuery>,
) -> AppResult<Json<HistoryResponse>> {
    let host_id = validate_host_id(&host_id)?;
    if query.from.zip(query.to).is_some_and(|(from, to)| from > to) {
        return Err(AppError::BadRequest(
            "history from must not be after to".to_string(),
        ));
    }
    if database::get_monitored_host(state.db().as_ref(), &host_id)
        .await
        .map_err(AppError::Anyhow)?
        .is_none()
    {
        return Err(AppError::NotFound("monitored host not found".to_string()));
    }
    let points = database::monitoring_history(
        state.db().as_ref(),
        &host_id,
        query.from,
        query.to,
        query.limit.unwrap_or(300).clamp(1, 1000),
    )
    .await
    .map_err(AppError::Anyhow)?
    .into_iter()
    .map(|stored| history_point(stored.report, stored.received_at))
    .collect();
    Ok(Json(HistoryResponse { host_id, points }))
}

fn host_summary(stored: database::StoredHost) -> HostSummary {
    let metrics = stored
        .latest
        .as_ref()
        .map(AgentReport::metric_summary)
        .unwrap_or_default();
    let status = host_status(stored.last_seen_at, stored.latest_interval_seconds);
    HostSummary {
        id: stored.identity.id,
        name: stored.identity.name,
        os: stored.identity.os,
        os_version: stored.identity.os_version,
        kernel_version: stored.identity.kernel_version,
        arch: stored.identity.arch,
        agent_version: stored.identity.agent_version,
        registered_at: stored.registered_at,
        last_seen_at: stored.last_seen_at,
        latest_collected_at: stored.latest_collected_at,
        status,
        capabilities: stored.capabilities,
        cpu_usage_percent: metrics.cpu_usage_percent,
        memory_usage_percent: metrics.memory_usage_percent,
        network_received_bytes_per_second: metrics.network_received_bytes_per_second,
        network_transmitted_bytes_per_second: metrics.network_transmitted_bytes_per_second,
        disk_read_bytes_per_second: metrics.disk_read_bytes_per_second,
        disk_written_bytes_per_second: metrics.disk_written_bytes_per_second,
        max_temperature_celsius: metrics.max_temperature_celsius,
        gpu_utilization_percent: metrics.gpu_utilization_percent,
        gpu_memory_usage_percent: metrics.gpu_memory_usage_percent,
    }
}

fn history_point(report: AgentReport, received_at: chrono::DateTime<Utc>) -> HistoryPoint {
    let MetricSummary {
        cpu_usage_percent,
        memory_usage_percent,
        network_received_bytes_per_second,
        network_transmitted_bytes_per_second,
        disk_read_bytes_per_second,
        disk_written_bytes_per_second,
        max_temperature_celsius,
        gpu_utilization_percent,
        gpu_memory_usage_percent,
    } = report.metric_summary();
    HistoryPoint {
        report_id: report.report_id,
        collected_at: report.collected_at,
        received_at,
        cpu_usage_percent,
        memory_usage_percent,
        network_received_bytes_per_second,
        network_transmitted_bytes_per_second,
        disk_read_bytes_per_second,
        disk_written_bytes_per_second,
        max_temperature_celsius,
        gpu_utilization_percent,
        gpu_memory_usage_percent,
    }
}

fn host_status(last_seen: chrono::DateTime<Utc>, interval: Option<f64>) -> String {
    let age = (Utc::now() - last_seen).num_seconds().max(0) as f64;
    let interval = interval.unwrap_or(10.0).clamp(1.0, 3600.0);
    if age <= (interval * 3.0).max(30.0) {
        "online"
    } else if age <= (interval * 12.0).max(300.0) {
        "stale"
    } else {
        "offline"
    }
    .to_string()
}

async fn require_database(state: &AppState) -> AppResult<()> {
    database::ping(state.db().as_ref())
        .await
        .map_err(|_| AppError::DatabaseUnavailable("database is unavailable".to_string()))
}

fn require_agent_https(state: &AppState, headers: &HeaderMap) -> AppResult<()> {
    if state.settings.production
        && headers
            .get("x-forwarded-proto")
            .and_then(|value| value.to_str().ok())
            != Some("https")
    {
        return Err(AppError::Forbidden(
            "agent API is only available through the HTTPS reverse proxy".to_string(),
        ));
    }
    Ok(())
}

fn bearer_token(headers: &HeaderMap) -> Option<&str> {
    let value = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    let (scheme, token) = value.split_once(' ')?;
    (scheme.eq_ignore_ascii_case("bearer") && !token.is_empty()).then_some(token)
}

fn token_hash(token: &str) -> String {
    Sha256::digest(token.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn new_agent_token() -> String {
    format!(
        "uca_{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    )
}

fn validate_host_id(value: &str) -> AppResult<String> {
    uuid::Uuid::parse_str(value)
        .map(|value| value.to_string())
        .map_err(|_| AppError::BadRequest("host id must be a UUID".to_string()))
}

async fn check_registration_rate(state: &AppState) -> AppResult<()> {
    let now = Instant::now();
    let mut attempts = state.agents.registration_attempts.lock().await;
    attempts.retain(|attempt| now.duration_since(*attempt) < Duration::from_secs(60));
    if attempts.len() >= MAX_REGISTRATIONS_PER_MINUTE {
        return Err(AppError::TooManyRequests(
            "agent registration rate limit exceeded".to_string(),
        ));
    }
    attempts.push(now);
    Ok(())
}
