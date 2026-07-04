//! Sunshine 多主机管理 handler。
//!
//! 路由结构：
//!   GET  /hosts                         列出所有主机
//!   POST /hosts                         新建主机
//!   PUT  /hosts/{id}                    更新主机
//!   DELETE /hosts/{id}                  删除主机
//!   GET  /hosts/{id}/status             TCP 可达性检测
//!   POST /hosts/{id}/wake               Wake-on-LAN
//!   GET  /hosts/{id}/logs               读本地日志文件
//!   GET  /hosts/{id}/apps               Sunshine API 代理（以下同）
//!   ...（其余接口均在 /hosts/{id}/ 前缀下）
//!
//! # 代理模式说明
//!
//! `/hosts/{id}/apps` 等接口是"透明代理"：
//! 前端发请求给 union，union 找到对应主机的配置，
//! 用存储的用户名/密码向 Sunshine 发起认证请求，再把结果转发给前端。
//! 这样前端不需要存储 Sunshine 密码，也不存在跨域问题。

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::{HeaderValue, header},
    response::{IntoResponse, Response},
    routing::{delete, get, post, put},
};
use futures_util::{StreamExt, stream};
use serde_json::Value;

use crate::{
    app_config::SunshineHostConfig,
    database,
    domain::{
        LogsResponse, SunshineClientUpdateRequest, SunshineCoverUploadRequest, SunshineHostInfo,
        SunshineHostSaveRequest, SunshinePinRequest, SunshineStatus, SunshineUnpairRequest,
        WakeResponse,
    },
    error::{AppError, AppResult},
    network, service_manager,
    state::AppState,
    sunshine, wol,
};

use super::LogQuery;

mod common;
mod hosts;
mod proxy;

use hosts::*;
use proxy::*;

pub(super) fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/api/services/sunshine/hosts",
            get(list_hosts).post(create_host),
        )
        .route(
            "/api/services/sunshine/hosts/{id}",
            put(update_host).delete(delete_host),
        )
        .route("/api/services/sunshine/hosts/{id}/status", get(host_status))
        .route("/api/services/sunshine/hosts/{id}/wake", post(host_wake))
        .route("/api/services/sunshine/hosts/{id}/logs", get(host_logs))
        .route(
            "/api/services/sunshine/hosts/{id}/apps",
            get(apps_list).post(apps_save),
        )
        .route(
            "/api/services/sunshine/hosts/{id}/apps/close",
            post(apps_close),
        )
        .route(
            "/api/services/sunshine/hosts/{id}/apps/{index}",
            delete(apps_delete),
        )
        .route(
            "/api/services/sunshine/hosts/{id}/clients",
            get(clients_list),
        )
        .route(
            "/api/services/sunshine/hosts/{id}/clients/unpair",
            post(clients_unpair),
        )
        .route(
            "/api/services/sunshine/hosts/{id}/clients/unpair-all",
            post(clients_unpair_all),
        )
        .route(
            "/api/services/sunshine/hosts/{id}/clients/update",
            post(clients_update),
        )
        .route(
            "/api/services/sunshine/hosts/{id}/config",
            get(config_get).post(config_save),
        )
        .route(
            "/api/services/sunshine/hosts/{id}/config/locale",
            get(config_locale),
        )
        .route("/api/services/sunshine/hosts/{id}/api-logs", get(api_logs))
        .route("/api/services/sunshine/hosts/{id}/pin", post(pin))
        .route("/api/services/sunshine/hosts/{id}/password", post(password))
        .route("/api/services/sunshine/hosts/{id}/restart", post(restart))
        .route(
            "/api/services/sunshine/hosts/{id}/reset-display",
            post(reset_display),
        )
        .route(
            "/api/services/sunshine/hosts/{id}/covers/{index}",
            get(cover_get),
        )
        .route(
            "/api/services/sunshine/hosts/{id}/covers/upload",
            post(cover_upload),
        )
}
