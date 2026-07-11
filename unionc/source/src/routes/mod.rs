//! HTTP 路由装配。
//!
//! 各业务模块声明自己的 URL 与 handler 映射；本模块只组合路由并安装全局中间件。

mod access_control;
mod auth;
mod monitoring;
mod sunshine;
mod system;

use axum::{Router, extract::DefaultBodyLimit, middleware};
use serde::Deserialize;
use tower_http::{
    request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer},
    trace::TraceLayer,
};

use crate::state::AppState;

/// 日志接口查询参数，例如 `?lines=300`。
#[derive(Debug, Deserialize)]
pub(super) struct LogQuery {
    pub lines: Option<usize>,
}

fn api_routes() -> Router<AppState> {
    Router::new()
        .merge(auth::router())
        .merge(system::router())
        .merge(monitoring::console_router())
        .merge(sunshine::router())
}

/// 构造整个 HTTP API 路由树。
pub fn router(state: AppState) -> Router {
    let console = api_routes().layer(middleware::from_fn_with_state(
        state.clone(),
        access_control::require_auth,
    ));
    Router::new()
        .merge(console)
        .merge(monitoring::agent_router())
        .layer(DefaultBodyLimit::max(10 * 1024 * 1024))
        .layer(PropagateRequestIdLayer::x_request_id())
        .layer(TraceLayer::new_for_http())
        .layer(SetRequestIdLayer::x_request_id(MakeRequestUuid))
        .with_state(state)
}
