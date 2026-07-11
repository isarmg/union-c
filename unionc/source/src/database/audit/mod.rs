//! 审计日志持久化与请求上下文。

use super::DbPool;

#[derive(Clone)]
pub struct AuditContext {
    pub actor: String,
    pub request_id: Option<String>,
}

tokio::task_local! { static AUDIT_CONTEXT: AuditContext; }

pub async fn with_audit_context<F>(context: AuditContext, future: F) -> F::Output
where
    F: std::future::Future,
{
    AUDIT_CONTEXT.scope(context, future).await
}

mod operations;
pub use operations::{insert_audit, prune_audit_history};
