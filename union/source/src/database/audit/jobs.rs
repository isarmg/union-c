//! 后台任务状态持久化。

use super::*;

// ─── 后台任务 ─────────────────────────────────────────────────────────────────
// "Job"（后台任务）是指异步运行的长时间操作，如博客构建、文件处理等。
// 记录在数据库中方便监控、查询历史结果和调试失败原因。

/// 创建一条新的后台任务记录（状态为 "running"）。
///
/// `id` 通常是调用方生成的 UUID，用于后续更新任务状态。
/// `kind` 标识任务类型（如 "blog.build"），用于分类查询。
pub async fn create_job(pool: &DbPool, id: &str, kind: &str) -> anyhow::Result<()> {
    query(
        r#"
        INSERT INTO jobs (id, kind, status)
        VALUES ($1, $2, 'running')
        "#,
    )
    .bind(id)
    .bind(kind)
    .execute(pool)
    .await?;
    Ok(())
}

/// 完成后台任务，写入最终结果。
///
/// `status` 通常为 "success" 或 "failed"。
/// `exit_code` 是外部进程的退出码（0 表示成功，非 0 表示失败）。
/// `duration_ms` 是任务耗时（毫秒），用于性能分析。
/// `log_path` 是任务日志文件路径（方便后续查看详细输出）。
pub async fn finish_job(
    pool: &DbPool,
    id: &str,
    status: &str,
    exit_code: Option<i32>,
    duration_ms: i64,
    log_path: Option<&str>,
) -> anyhow::Result<()> {
    query(
        r#"
        UPDATE jobs
        SET status      = $1,
            exit_code   = $2,
            duration_ms = $3,
            log_path    = $4,
            finished_at = NOW()
        WHERE id = $5
        "#,
    )
    .bind(status)
    .bind(exit_code)
    .bind(duration_ms)
    .bind(log_path)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}
