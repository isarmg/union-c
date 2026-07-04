//! ram 文件服务 handler。
//!
//! ram 是一个开源的文件服务器程序，负责提供文件的上传、下载、浏览功能。
//! union 通过 `service_manager` 模块管理 ram 进程（启动/停止/配置），
//! 并在这里暴露 REST API 供前端控制。

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, post},
};

use crate::{
    domain::{
        ActionResponse, LogsResponse, RamAuthUpdateRequest, RamCommandResponse, RamConfigResponse,
        RamEntryResponse, RamHealthResponse,
    },
    error::AppResult,
    ram_auth, ram_instances, service_manager,
    state::AppState,
};

use super::LogQuery;

/// 目录浏览的查询参数，`path` 为可选的相对路径（不提供则返回根目录）。
#[derive(Debug, serde::Deserialize)]
pub(super) struct RamEntryQuery {
    pub path: Option<String>,
}

pub(super) fn router() -> Router<AppState> {
    Router::new()
        .route("/api/services/ram/start", post(start))
        .route("/api/services/ram/stop", post(stop))
        .route("/api/services/ram/restart", post(restart))
        .route("/api/services/ram/config", get(config))
        .route("/api/services/ram/command", get(command))
        .route("/api/services/ram/auth", get(get_auth).post(update_auth))
        .route("/api/services/ram/health", get(health))
        .route("/api/services/ram/entry", get(entry))
        .route("/api/services/ram/logs", get(logs))
        .route(
            "/api/services/ram/instances",
            get(instances).post(create_instance),
        )
        .route(
            "/api/services/ram/instances/{id}",
            axum::routing::put(update_instance).delete(delete_instance),
        )
        .route(
            "/api/services/ram/instances/{id}/auth",
            get(instance_auth).post(update_instance_auth),
        )
}

pub(super) async fn instances(
    State(state): State<AppState>,
) -> AppResult<Json<Vec<crate::domain::RamInstanceInfo>>> {
    Ok(Json(ram_instances::list(&state).await?))
}
pub(super) async fn create_instance(
    State(state): State<AppState>,
    Json(req): Json<crate::domain::RamInstanceSaveRequest>,
) -> AppResult<Json<crate::domain::RamInstanceInfo>> {
    Ok(Json(ram_instances::create(&state, req).await?))
}
pub(super) async fn update_instance(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<crate::domain::RamInstanceSaveRequest>,
) -> AppResult<Json<crate::domain::RamInstanceInfo>> {
    Ok(Json(ram_instances::update(&state, &id, req).await?))
}
pub(super) async fn delete_instance(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<StatusCode> {
    ram_instances::delete(&state, &id).await?;
    Ok(StatusCode::NO_CONTENT)
}
pub(super) async fn instance_auth(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<crate::domain::RamAuthResponse>> {
    Ok(Json(
        ram_auth::current_auth_for(&state, &ram_instances::service_key(&id)).await?,
    ))
}
pub(super) async fn update_instance_auth(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<RamAuthUpdateRequest>,
) -> AppResult<Json<crate::domain::RamAuthUpdateResponse>> {
    Ok(Json(
        ram_auth::update_auth_for(&state, req, &ram_instances::service_key(&id), false).await?,
    ))
}

/// 启动 ram 服务进程。
///
/// 返回 `ActionResponse`，包含操作是否成功以及相关消息。
pub(super) async fn start(State(state): State<AppState>) -> AppResult<Json<ActionResponse>> {
    Ok(Json(service_manager::start_ram(&state).await?))
}

/// 停止 ram 服务进程。
pub(super) async fn stop(State(state): State<AppState>) -> AppResult<Json<ActionResponse>> {
    Ok(Json(service_manager::stop_ram(&state).await?))
}

/// 重启 ram 服务进程（先停止再启动）。
///
/// 注意：这里是顺序执行的两个 await，先等 stop 完成再执行 start，
/// 确保旧进程完全退出后才启动新进程，避免端口冲突。
pub(super) async fn restart(State(state): State<AppState>) -> AppResult<Json<ActionResponse>> {
    Ok(Json(service_manager::restart_ram(&state).await?))
}

/// 获取 ram 的当前配置（如监听端口、根目录、启用的功能等）。
pub(super) async fn config(State(state): State<AppState>) -> AppResult<Json<RamConfigResponse>> {
    Ok(Json(service_manager::ram_config(&state).await?))
}

/// 获取实际运行的 ram 命令行（用于调试，展示 ram 进程启动时使用的完整参数）。
pub(super) async fn command(State(state): State<AppState>) -> AppResult<Json<RamCommandResponse>> {
    Ok(Json(service_manager::ram_command(&state).await?))
}

/// 获取 ram 的认证配置（哪些用户有哪些路径的读/写权限）。
pub(super) async fn get_auth(
    State(state): State<AppState>,
) -> AppResult<Json<crate::domain::RamAuthResponse>> {
    Ok(Json(ram_auth::current_auth(&state).await?))
}

/// 更新 ram 的认证配置。
///
/// ram 支持基于路径的细粒度权限控制，如：
/// - 匿名用户只读访问 `/public`
/// - 认证用户读写访问 `/private`
///
/// 这个接口允许前端修改这些权限规则。
pub(super) async fn update_auth(
    State(state): State<AppState>,
    payload: Json<RamAuthUpdateRequest>,
) -> AppResult<Json<crate::domain::RamAuthUpdateResponse>> {
    Ok(Json(ram_auth::update_auth(&state, payload.0).await?))
}

/// 检查 ram 服务的运行健康状态（进程是否在运行、端口是否可访问等）。
///
/// 注意：这个函数返回 `Json<RamHealthResponse>` 而不是 `AppResult<...>`，
/// 意味着它不会返回错误——即使 ram 没有运行，也返回包含状态信息的 JSON，
/// 而不是返回 HTTP 错误码。这样前端可以始终收到健康状态数据。
pub(super) async fn health(State(state): State<AppState>) -> Json<RamHealthResponse> {
    Json(service_manager::ram_health(&state).await)
}

/// 列出指定路径下的文件和目录（通过 ram API 或直接文件系统读取）。
///
/// `Query(query)` 提取 URL 中的 `?path=xxx` 参数，不提供则查询根目录。
pub(super) async fn entry(
    State(state): State<AppState>,
    Query(query): Query<RamEntryQuery>,
) -> AppResult<Json<RamEntryResponse>> {
    Ok(Json(service_manager::ram_entry(&state, query.path).await?))
}

/// 读取 ram 的运行日志（从日志文件尾部读取指定行数）。
///
/// `LogQuery` 包含 `lines` 参数，默认返回最后 200 行，最多 1000 行。
/// 日志路径来自应用设置（`state.settings.ram.log_path`）。
pub(super) async fn logs(
    State(state): State<AppState>,
    Query(query): Query<LogQuery>,
) -> AppResult<Json<LogsResponse>> {
    // `.unwrap_or(200)` 如果 `lines` 参数未提供（None），默认 200 行
    // `.min(1000)` 限制最大行数，防止一次读取过多内容占用过多内存
    let lines = query.lines.unwrap_or(200).min(1000);
    let path = &state.settings.ram.log_path;
    Ok(Json(LogsResponse {
        path: path.to_string_lossy().to_string(),
        lines: service_manager::tail_lines(path, lines)?, // 读取日志文件末尾 N 行
    }))
}
