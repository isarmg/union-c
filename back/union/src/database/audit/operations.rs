//! 审计记录、服务事件和期望状态。

use super::*;

// ─── 审计日志 ─────────────────────────────────────────────────────────────────

/// 写入一条审计日志记录。
///
/// # 参数说明
///
/// - `action`：操作类型，建议用 "模块.操作" 格式（如 "auth.login"、"sunshine.wake"）
/// - `target`：操作对象（如用户名、主机名）
/// - `detail`：可选的详细描述（如包含参数的完整描述）
///
/// `created_at` 由数据库自动填充（DEFAULT NOW()），不需要应用层传入。
pub async fn insert_audit(
    pool: &DbPool,
    action: &str,
    target: &str,
    detail: Option<&str>,
) -> anyhow::Result<()> {
    let context = AUDIT_CONTEXT
        .try_with(Clone::clone)
        .unwrap_or(AuditContext {
            actor: "system".to_string(),
            request_id: None,
        });
    query(
        r#"
        INSERT INTO audit_logs (action, target, detail, actor, request_id)
        VALUES ($1, $2, $3, $4, $5)
        "#,
    )
    .bind(action)
    .bind(target)
    .bind(detail) // `Option<&str>` 会被 sqlx 映射为 SQL NULL（如果为 None）
    .bind(context.actor)
    .bind(context.request_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// 写入服务事件（记录服务的启动、停止、错误等生命周期事件）。
///
/// 与审计日志的区别：审计日志记录用户操作，服务事件记录系统/进程级别的状态变化。
pub async fn service_event(
    pool: &DbPool,
    service_name: &str,
    action: &str,          // 如 "start"、"stop"、"crash"
    message: Option<&str>, // 可选的附加信息（如崩溃原因、退出码）
) -> anyhow::Result<()> {
    query(
        r#"
        INSERT INTO service_events (service_name, action, message)
        VALUES ($1, $2, $3)
        "#,
    )
    .bind(service_name)
    .bind(action)
    .bind(message)
    .execute(pool)
    .await?;
    Ok(())
}

/// 删除超过保留期的操作历史，防止数据库无限增长。
pub async fn prune_operational_history(pool: &DbPool, retention_days: i64) -> anyhow::Result<u64> {
    let retention_days = retention_days.clamp(7, 3650);
    let mut removed = 0;
    for statement in [
        "DELETE FROM audit_logs WHERE created_at < NOW() - ($1 * INTERVAL '1 day')",
        "DELETE FROM service_events WHERE created_at < NOW() - ($1 * INTERVAL '1 day')",
        "DELETE FROM job_logs WHERE job_id IN (SELECT id FROM jobs WHERE created_at < NOW() - ($1 * INTERVAL '1 day'))",
        "DELETE FROM jobs WHERE created_at < NOW() - ($1 * INTERVAL '1 day')",
    ] {
        removed += query(statement)
            .bind(retention_days)
            .execute(pool)
            .await?
            .rows_affected();
    }
    Ok(removed)
}

/// 更新服务的期望状态（desired state），用于持久化"用户希望服务处于什么状态"。
///
/// # 期望状态 vs 实际状态
///
/// "期望状态"（desired_state）是控制器模式的核心概念：
/// - `desired_state = "running"`：用户希望服务运行（重启后自动启动）
/// - `desired_state = "stopped"`：用户希望服务停止（重启后不自动启动）
///
/// 实际状态由进程是否运行决定，期望状态存在数据库中持久化用户意图。
pub async fn set_service_desired_state(
    pool: &DbPool,
    service_name: &str,
    desired_state: &str,
) -> anyhow::Result<()> {
    query(
        r#"
        UPDATE services
        SET desired_state = $1, updated_at = NOW()
        WHERE name = $2
        "#,
    )
    .bind(desired_state)
    .bind(service_name)
    .execute(pool)
    .await?;
    Ok(())
}

/// 读取服务的期望状态，用于union重启后恢复托管服务。
pub async fn service_desired_state(
    pool: &DbPool,
    service_name: &str,
) -> anyhow::Result<Option<String>> {
    let row = query("SELECT desired_state FROM services WHERE name = $1")
        .bind(service_name)
        .fetch_optional(pool)
        .await?;
    row.map(|row| row.try_get("desired_state").map_err(Into::into))
        .transpose()
}
