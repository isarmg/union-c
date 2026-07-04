//! 审计日志、服务事件、后台任务、ram 服务账号权限。
//!
//! # 审计日志模式（Audit Log Pattern）
//!
//! 审计日志是一种"只追加"（append-only）的记录，用于追踪系统中发生了什么操作：
//! - 谁（`target`）做了什么（`action`）
//! - 什么时候（`created_at`，由数据库自动记录）
//! - 详细信息（`detail`，可选）
//!
//! 与普通日志（文件日志）不同，审计日志存在数据库中，便于查询和分析。
//! 典型用途：安全审计、操作追溯、故障排查。
//!
//! # 服务账号与权限
//!
//! ram 使用"服务账号"来控制文件访问权限：
//! 每个账号（匿名或具名）可以对特定路径拥有读取（"read"）或读写（"readwrite"）权限。
//! 这个模块负责这些账号配置的数据库 CRUD 操作。

use sqlx_core::{query::query, row::Row};

use super::DbPool;

#[derive(Clone)]
pub struct AuditContext {
    pub actor: String,
    pub request_id: Option<String>,
}

tokio::task_local! {
    static AUDIT_CONTEXT: AuditContext;
}

pub async fn with_audit_context<F>(context: AuditContext, future: F) -> F::Output
where
    F: std::future::Future,
{
    AUDIT_CONTEXT.scope(context, future).await
}

mod accounts;
mod jobs;
mod operations;

pub use accounts::{
    ServiceAccountInput, ServiceAccountRecord, ServicePermissionInput, count_service_accounts,
    replace_service_accounts, service_accounts,
};
pub use jobs::{create_job, finish_job};
pub use operations::{
    insert_audit, prune_operational_history, service_desired_state, service_event,
    set_service_desired_state,
};
