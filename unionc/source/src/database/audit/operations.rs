use sqlx_core::query::query;

use super::*;

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
    query("INSERT INTO audit_logs(action,target,detail,actor,request_id) VALUES($1,$2,$3,$4,$5)")
        .bind(action)
        .bind(target)
        .bind(detail)
        .bind(context.actor)
        .bind(context.request_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn prune_audit_history(pool: &DbPool, retention_days: i64) -> anyhow::Result<u64> {
    Ok(
        query("DELETE FROM audit_logs WHERE created_at < NOW() - ($1 * INTERVAL '1 day')")
            .bind(retention_days.clamp(7, 3650))
            .execute(pool)
            .await?
            .rows_affected(),
    )
}
