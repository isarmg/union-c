//! 健康检查、系统资源和 SSE 事件流 handler。
//!
//! # SSE（Server-Sent Events）原理
//!
//! SSE 是一种从服务器向浏览器单向推送数据的技术：
//! - 客户端（浏览器）通过 `EventSource` API 建立一个长连接
//! - 服务器持续向这个连接写入数据，格式如：`data: {"status":"ok"}\n\n`
//! - 浏览器自动触发 `onmessage` 事件，无需客户端轮询
//!
//! SSE 比 WebSocket 简单，适合管理台单向接收服务状态更新。

use std::{
    convert::Infallible,
    time::{Duration, Instant},
};

use async_stream::stream;
use axum::{
    Json, Router,
    extract::State,
    response::sse::{Event, KeepAlive, Sse},
    routing::{get, post},
};
use chrono::Utc;

use crate::{
    domain::{
        DatabaseConfigResponse, EventPayload, HealthResponse, ReadinessResponse, ServiceStatus,
        SseTicketResponse, UpdateDatabaseConfigRequest,
    },
    error::AppResult,
    service_manager,
    state::AppState,
    system,
};

pub(super) fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(root))
        .route("/api/health", get(health))
        .route("/api/ready", get(ready))
        .route(
            "/api/settings/database",
            get(database_config).put(update_database_config),
        )
        .route("/api/services", get(services))
        .route("/api/system/resources", get(resources))
        .route("/api/events", get(events))
        .route("/api/events/ticket", post(issue_sse_ticket))
}

pub(super) async fn database_config(State(state): State<AppState>) -> Json<DatabaseConfigResponse> {
    let config = state.auth.local_config.read().await;
    let connected = crate::database::ping(state.db().as_ref()).await.is_ok();
    Json(DatabaseConfigResponse {
        configured: !config.database_url.trim().is_empty(),
        database_url: redact_database_url(&config.database_url),
        connected,
        restart_required: false,
    })
}

pub(super) async fn update_database_config(
    State(state): State<AppState>,
    Json(payload): Json<UpdateDatabaseConfigRequest>,
) -> AppResult<Json<DatabaseConfigResponse>> {
    let url = payload.database_url.trim();
    if url.is_empty() {
        return Err(crate::error::AppError::BadRequest(
            "数据库连接地址不能为空".to_string(),
        ));
    }
    let parsed = url::Url::parse(url)
        .map_err(|_| crate::error::AppError::BadRequest("数据库连接地址格式无效".to_string()))?;
    if !matches!(parsed.scheme(), "postgres" | "postgresql") {
        return Err(crate::error::AppError::BadRequest(
            "仅支持 PostgreSQL 连接地址".to_string(),
        ));
    }
    let mut candidate = (*state.settings).clone();
    candidate.database.url = url.to_string();
    let pool = crate::database::connect(&candidate)
        .await
        .map_err(crate::error::AppError::Anyhow)?;
    crate::database::migrate(&pool)
        .await
        .map_err(crate::error::AppError::Anyhow)?;
    let loaded_settings = crate::database::load_or_seed_app_settings(&pool, &candidate)
        .await
        .map_err(crate::error::AppError::Anyhow)?;
    crate::ram_auth::ensure_seeded(&pool, &loaded_settings)
        .await
        .map_err(crate::error::AppError::Anyhow)?;

    let mut config = state.auth.local_config.write().await;
    config.database_url = url.to_string();
    crate::app_config::save_local_config(&config).map_err(crate::error::AppError::Anyhow)?;
    drop(config);

    // 迁移和配置写入全部成功后再原子切换，失败不会破坏当前连接。
    state.replace_db(pool);
    *state.hosts.proxmox.write().await = loaded_settings.proxmox.hosts;
    *state.hosts.sunshine.write().await = loaded_settings.sunshine.hosts;
    Ok(Json(DatabaseConfigResponse {
        configured: true,
        database_url: redact_database_url(url),
        connected: true,
        restart_required: false,
    }))
}

fn redact_database_url(value: &str) -> String {
    let Ok(mut url) = url::Url::parse(value) else {
        return value.to_string();
    };
    if url.password().is_some() {
        let _ = url.set_password(Some("********"));
    }
    url.to_string()
}

/// 为 SSE 连接签发一个短期（60 秒）有效的单次访问票据（ticket）。
///
/// # 为什么需要 ticket？
///
/// 浏览器的 `EventSource` API 不支持自定义请求头（如 `Authorization: Bearer <token>`），
/// 所以无法用通常的 Bearer Token 方式验证身份。
///
/// 解决方案：先用正常的认证请求获取一个临时 ticket（UUID），
/// 然后通过 URL 查询参数传给 SSE 端点：`GET /api/events?ticket=<uuid>`
///
/// # 安全设计
///
/// - ticket 有效期只有 60 秒（足够客户端立即使用）
/// - ticket 是随机 UUID，无法猜测
/// - 服务端验证成功后立即删除 ticket，使其只能使用一次
/// - 限制泄露风险：即使 URL 被日志记录，60 秒后 ticket 自动失效
pub(super) async fn issue_sse_ticket(
    State(state): State<AppState>,
) -> AppResult<Json<SseTicketResponse>> {
    let ticket = uuid::Uuid::new_v4().to_string(); // 生成随机 UUID 作为 ticket
    let mut tickets = state.auth.sse_tickets.lock().await;
    // 清理超过 60 秒未使用的过期 ticket。
    // `retain` 保留返回 true 的元素，这里保留"距今不超过 60 秒"的 ticket。
    // 这是一个懒清理策略：在写入时顺便清理，避免 HashMap 无限增长。
    tickets.retain(|_, issued_at: &mut Instant| issued_at.elapsed() < Duration::from_secs(60));
    tickets.insert(ticket.clone(), Instant::now());
    Ok(Json(SseTicketResponse { ticket }))
}

/// 根路由，返回 API 简介文字（主要用于快速验证服务是否运行）。
pub(super) async fn root() -> &'static str {
    "Union API. Try GET /api/health."
}

/// 返回服务健康状态，包含版本号和运行时长（秒）。
///
/// 这个接口不需要认证，常用于监控系统的存活探针（liveness probe）。
pub(super) async fn health(State(state): State<AppState>) -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(), // 编译时从 Cargo.toml 读取版本号
        uptime_seconds: (Utc::now() - state.started_at).num_seconds(),
    })
}

/// 就绪探针同时验证数据库和运行数据目录。
pub(super) async fn ready(
    State(state): State<AppState>,
) -> (axum::http::StatusCode, Json<ReadinessResponse>) {
    let database = crate::database::ping(state.db().as_ref()).await.is_ok();
    let database_configured = !state
        .auth
        .local_config
        .read()
        .await
        .database_url
        .trim()
        .is_empty();
    let data_directory =
        state.settings.paths.data_dir.is_dir() && state.settings.paths.blog_export_dir.is_dir();
    // 未配置数据库是受支持的引导状态；配置存在但连接失败才是不就绪。
    let ready = data_directory && (database || !database_configured);
    (
        if ready {
            axum::http::StatusCode::OK
        } else {
            axum::http::StatusCode::SERVICE_UNAVAILABLE
        },
        Json(ReadinessResponse {
            status: if ready { "ready" } else { "not-ready" }.to_string(),
            database,
            data_directory,
        }),
    )
}

/// 返回所有受管服务的当前运行状态列表（ram、blog 等）。
pub(super) async fn services(State(state): State<AppState>) -> AppResult<Json<Vec<ServiceStatus>>> {
    Ok(Json(service_manager::all_services(&state).await?))
}

/// 返回当前系统资源快照（CPU、内存、磁盘、网络吞吐）。
///
/// 注意：没有 `State` 参数，直接调用 `system::collect_resources()`，
/// 这个函数每次调用都即时采集，不依赖全局状态（网络采样除外，它用静态变量）。
pub(super) async fn resources() -> Json<crate::domain::SystemResources> {
    Json(system::collect_resources())
}

/// SSE 服务状态推送流：每 5 秒向客户端推送一次所有服务的运行状态。
///
/// # axum SSE 的工作原理
///
/// `Sse::new(stream)` 接受一个实现了 `Stream<Item = Result<Event, Infallible>>` 的流。
/// `Event` 是一个 SSE 帧，包含：
/// - `.event("status")` — 事件名称（前端 `addEventListener("status", ...)` 监听）
/// - `.data(string)` — 事件数据（通常是 JSON 字符串）
///
/// `async_stream::stream!` 宏允许用类似同步的写法创建异步流（yield + await）：
/// - `yield Ok(event)` 向流中发送一个事件帧
/// - `tokio::time::sleep(...).await` 等待 5 秒（异步等待，不阻塞线程）
///
/// `KeepAlive::default()` 会定期发送 SSE 心跳注释（`: ping`），
/// 防止代理服务器（如 nginx）因为连接长时间没有数据而关闭连接。
///
/// # 错误类型 `Infallible`
///
/// `Infallible` 是 Rust 标准库中表示"永不发生"的错误类型。
/// 这里 `yield Ok(...)` 保证不会产生错误，所以用 `Infallible` 作为错误类型。
pub(super) async fn events(
    State(state): State<AppState>,
) -> Sse<impl futures_core::Stream<Item = Result<Event, Infallible>>> {
    // `stream!` 宏生成一个 `async_stream::AsyncStream`，可以被 `Sse` 消费
    let events = stream! {
        loop {
            // 每次循环采集所有服务状态，失败时回退为空列表（不终止流）
            let services = service_manager::all_services(&state).await.unwrap_or_default();
            let payload = EventPayload {
                kind: "service-status".to_string(),
                generated_at: Utc::now().to_rfc3339(), // RFC 3339 格式时间戳，如 "2024-01-01T00:00:00Z"
                services,
            };
            // 序列化为 JSON，失败时退化为空 JSON 对象（避免流中断）
            let data = serde_json::to_string(&payload).unwrap_or_else(|_| "{}".to_string());
            // `yield` 向 SSE 流发送一个事件，前端通过 `eventSource.addEventListener("status", ...)` 监听
            yield Ok(Event::default().event("status").data(data));
            // 等待 5 秒再发送下一次数据（异步等待，此期间线程可以处理其他请求）
            tokio::time::sleep(Duration::from_secs(5)).await;
        }
    };
    Sse::new(events).keep_alive(KeepAlive::default()) // 启用 SSE 心跳，防止连接超时断开
}
