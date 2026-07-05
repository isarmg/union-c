//! 统一错误类型。
//!
//! Axum 的 handler 可以返回 `Result<T, AppError>`。当出现错误时，`IntoResponse`
//! 会把错误转换成统一 JSON 响应，前端就能稳定读取 `message` 字段。

use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use thiserror::Error;

use crate::{app_config::LocalConfigError, domain::ErrorResponse};

/// 项目内 handler 常用的结果类型别名。
pub type AppResult<T> = Result<T, AppError>;

/// 应用错误分类。
#[derive(Debug, Error)]
pub enum AppError {
    /// 请求参数不合法，返回 400。
    #[error("{0}")]
    BadRequest(String),
    /// 本地管理员配置校验失败，返回可细分机器码。
    #[error(transparent)]
    LocalConfig(#[from] LocalConfigError),
    /// 主机地址格式错误，返回稳定机器码 `invalid_host`。
    #[error("{0}")]
    InvalidHost(String),
    /// 认证失败，返回 401。
    #[error("unauthorized")]
    Unauthorized,
    /// 已认证但请求缺少必要的安全证明，返回 403。
    #[error("{0}")]
    Forbidden(String),
    /// 当前状态冲突，例如重复启动服务，返回 409。
    #[error("{0}")]
    Conflict(String),
    /// 请求过于频繁，返回 429。
    #[error("{0}")]
    TooManyRequests(String),
    /// 数据库尚未连接，业务接口暂不可用，返回 503。
    #[error("{0}")]
    ServiceUnavailable(String),
    /// PostgreSQL 未配置或当前不可达。
    #[error("{0}")]
    DatabaseUnavailable(String),
    /// 外部进程或依赖服务出错，返回 502。
    #[error("{0}")]
    Process(String),
    /// 管理员配置的上游 HTTP 服务错误；消息可安全返回给已认证控制台。
    #[error("{0}")]
    Upstream(String),
    /// 文件系统错误。
    #[error(transparent)]
    Io(#[from] std::io::Error),
    /// 数据库错误。
    #[error(transparent)]
    Sqlx(#[from] sqlx_core::Error),
    /// 其他使用 anyhow 传递的错误。
    #[error(transparent)]
    Anyhow(#[from] anyhow::Error),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = match &self {
            AppError::BadRequest(_) | AppError::LocalConfig(_) | AppError::InvalidHost(_) => {
                StatusCode::BAD_REQUEST
            }
            AppError::Unauthorized => StatusCode::UNAUTHORIZED,
            AppError::Forbidden(_) => StatusCode::FORBIDDEN,
            AppError::Conflict(_) => StatusCode::CONFLICT,
            AppError::TooManyRequests(_) => StatusCode::TOO_MANY_REQUESTS,
            AppError::ServiceUnavailable(_) | AppError::DatabaseUnavailable(_) => {
                StatusCode::SERVICE_UNAVAILABLE
            }
            AppError::Process(_) | AppError::Upstream(_) => StatusCode::BAD_GATEWAY,
            AppError::Io(_) | AppError::Sqlx(_) | AppError::Anyhow(_) => {
                StatusCode::INTERNAL_SERVER_ERROR
            }
        };

        // 内部错误记录完整信息用于调试，但对外只返回通用描述，不泄露路径或 SQL。
        let client_message = match &self {
            AppError::BadRequest(msg) => msg.clone(),
            AppError::LocalConfig(error) => error.to_string(),
            AppError::InvalidHost(msg) => msg.clone(),
            AppError::Unauthorized => "unauthorized".to_string(),
            AppError::Forbidden(msg) => msg.clone(),
            AppError::Conflict(msg) => msg.clone(),
            AppError::TooManyRequests(msg) => msg.clone(),
            AppError::ServiceUnavailable(msg) => msg.clone(),
            AppError::DatabaseUnavailable(msg) => msg.clone(),
            AppError::Process(_) => "upstream service error".to_string(),
            AppError::Upstream(msg) => msg.clone(),
            AppError::Io(_) => "storage error".to_string(),
            AppError::Sqlx(_) => "database error".to_string(),
            AppError::Anyhow(_) => "internal error".to_string(),
        };

        if status == StatusCode::INTERNAL_SERVER_ERROR {
            tracing::error!("internal error: {self}");
        } else if matches!(&self, AppError::Process(_) | AppError::Upstream(_)) {
            tracing::warn!("upstream/process error: {self}");
        }

        let body = Json(ErrorResponse {
            code: self.code().to_string(),
            error: status
                .canonical_reason()
                .unwrap_or("application error")
                .to_string(),
            message: client_message,
        });

        (status, body).into_response()
    }
}

impl AppError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::BadRequest(_) => "bad_request",
            Self::LocalConfig(error) => error.code(),
            Self::InvalidHost(_) => "invalid_host",
            Self::Unauthorized => "unauthorized",
            Self::Forbidden(_) => "forbidden",
            Self::Conflict(_) => "conflict",
            Self::TooManyRequests(_) => "too_many_requests",
            Self::ServiceUnavailable(_) => "service_unavailable",
            Self::DatabaseUnavailable(_) => "database_unavailable",
            Self::Process(_) => "process_error",
            Self::Upstream(_) => "upstream_error",
            Self::Io(_) => "storage_error",
            Self::Sqlx(_) => "database_error",
            Self::Anyhow(_) => "internal_error",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_codes_are_stable_and_message_independent() {
        assert_eq!(
            AppError::BadRequest("任意消息".into()).code(),
            "bad_request"
        );
        assert_eq!(AppError::Unauthorized.code(), "unauthorized");
        assert_eq!(AppError::InvalidHost("bad".into()).code(), "invalid_host");
        assert_eq!(
            AppError::LocalConfig(LocalConfigError::InvalidDatabaseUrl).code(),
            "local_config_database_url_invalid"
        );
        assert_eq!(
            AppError::Upstream("changed".into()).code(),
            "upstream_error"
        );
    }
}
