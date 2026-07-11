//! Persistence for read-only host metric reports.

use chrono::{DateTime, Utc};
use sqlx_core::{query::query, row::Row};

use crate::domain::{AgentReport, CapabilityReport, HostIdentity};

use super::DbPool;

#[derive(Debug)]
pub struct StoredHost {
    pub identity: HostIdentity,
    pub capabilities: Vec<CapabilityReport>,
    pub registered_at: DateTime<Utc>,
    pub last_seen_at: DateTime<Utc>,
    pub latest_collected_at: Option<DateTime<Utc>>,
    pub latest_interval_seconds: Option<f64>,
    pub latest: Option<AgentReport>,
}

#[derive(Debug)]
pub struct StoredHistoryPoint {
    pub report: AgentReport,
    pub received_at: DateTime<Utc>,
}

pub async fn register_monitoring_host(
    pool: &DbPool,
    host: &HostIdentity,
    enrollment_secret_hash: &str,
    token_hash: &str,
) -> anyhow::Result<bool> {
    let host_id = canonical_uuid(&host.id)?;
    let result = query(
        r#"
        INSERT INTO monitored_hosts(
            host_id,name,os,os_version,kernel_version,arch,agent_version,
            enrollment_secret_hash,agent_token_hash
        ) VALUES($1::uuid,$2,$3,$4,$5,$6,$7,$8,$9)
        ON CONFLICT(host_id) DO UPDATE SET
            name=EXCLUDED.name,
            os=EXCLUDED.os,
            os_version=EXCLUDED.os_version,
            kernel_version=EXCLUDED.kernel_version,
            arch=EXCLUDED.arch,
            agent_version=EXCLUDED.agent_version,
            agent_token_hash=EXCLUDED.agent_token_hash,
            last_seen_at=NOW()
        WHERE monitored_hosts.enrollment_secret_hash=EXCLUDED.enrollment_secret_hash
        RETURNING host_id
        "#,
    )
    .bind(host_id)
    .bind(host.name.trim())
    .bind(host.os.trim())
    .bind(host.os_version.as_deref())
    .bind(host.kernel_version.as_deref())
    .bind(host.arch.trim())
    .bind(host.agent_version.trim())
    .bind(enrollment_secret_hash)
    .bind(token_hash)
    .fetch_optional(pool)
    .await?;
    Ok(result.is_some())
}

pub async fn monitoring_host_for_token(
    pool: &DbPool,
    token_hash: &str,
) -> anyhow::Result<Option<String>> {
    let row =
        query("SELECT host_id::text AS host_id FROM monitored_hosts WHERE agent_token_hash=$1")
            .bind(token_hash)
            .fetch_optional(pool)
            .await?;
    row.map(|row| row.try_get("host_id").map_err(Into::into))
        .transpose()
}

pub async fn store_monitoring_report(
    pool: &DbPool,
    report: &AgentReport,
) -> anyhow::Result<(bool, DateTime<Utc>)> {
    let host_id = canonical_uuid(&report.host.id)?;
    let report_id = canonical_uuid(&report.report_id)?;
    let payload = serde_json::to_string(report)?;
    let capabilities = serde_json::to_string(&report.capabilities)?;
    let mut tx = pool.begin().await?;

    let updated = query(
        r#"
        UPDATE monitored_hosts SET
            name=$2,
            os=$3,
            os_version=$4,
            kernel_version=$5,
            arch=$6,
            agent_version=$7,
            capabilities=$8::jsonb,
            last_seen_at=NOW()
        WHERE host_id=$1::uuid
        "#,
    )
    .bind(&host_id)
    .bind(report.host.name.trim())
    .bind(report.host.os.trim())
    .bind(report.host.os_version.as_deref())
    .bind(report.host.kernel_version.as_deref())
    .bind(report.host.arch.trim())
    .bind(report.host.agent_version.trim())
    .bind(&capabilities)
    .execute(&mut *tx)
    .await?;
    if updated.rows_affected() != 1 {
        anyhow::bail!("monitoring host disappeared while storing report");
    }

    let inserted = query(
        r#"
        INSERT INTO agent_metric_reports(
            report_id,host_id,schema_version,collected_at,interval_seconds,payload
        ) VALUES($1::uuid,$2::uuid,$3,$4,$5,$6::jsonb)
        ON CONFLICT(report_id) DO NOTHING
        RETURNING received_at
        "#,
    )
    .bind(&report_id)
    .bind(&host_id)
    .bind(i32::from(report.schema_version))
    .bind(report.collected_at)
    .bind(report.interval_seconds)
    .bind(&payload)
    .fetch_optional(&mut *tx)
    .await?;

    let (accepted, received_at) = if let Some(row) = inserted {
        let received_at = row.try_get("received_at")?;
        query(
            r#"
            UPDATE monitored_hosts SET
                latest_report_id=$2::uuid,
                latest_collected_at=$3,
                latest_interval_seconds=$4,
                latest_report=$5::jsonb
            WHERE host_id=$1::uuid
              AND (latest_collected_at IS NULL OR latest_collected_at <= $3)
            "#,
        )
        .bind(&host_id)
        .bind(&report_id)
        .bind(report.collected_at)
        .bind(report.interval_seconds)
        .bind(&payload)
        .execute(&mut *tx)
        .await?;
        (true, received_at)
    } else {
        let row = query(
            "SELECT received_at FROM agent_metric_reports WHERE report_id=$1::uuid AND host_id=$2::uuid",
        )
        .bind(&report_id)
        .bind(&host_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| anyhow::anyhow!("report_id already belongs to another host"))?;
        (false, row.try_get("received_at")?)
    };
    tx.commit().await?;
    Ok((accepted, received_at))
}

pub async fn list_monitored_hosts(pool: &DbPool) -> anyhow::Result<Vec<StoredHost>> {
    let rows = query(&host_select("ORDER BY last_seen_at DESC, name"))
        .fetch_all(pool)
        .await?;
    rows.into_iter().map(stored_host_from_row).collect()
}

pub async fn get_monitored_host(
    pool: &DbPool,
    host_id: &str,
) -> anyhow::Result<Option<StoredHost>> {
    let host_id = canonical_uuid(host_id)?;
    let row = query(&host_select("WHERE host_id=$1::uuid"))
        .bind(host_id)
        .fetch_optional(pool)
        .await?;
    row.map(stored_host_from_row).transpose()
}

pub async fn monitoring_history(
    pool: &DbPool,
    host_id: &str,
    from: Option<DateTime<Utc>>,
    to: Option<DateTime<Utc>>,
    limit: i64,
) -> anyhow::Result<Vec<StoredHistoryPoint>> {
    let host_id = canonical_uuid(host_id)?;
    let rows = query(
        r#"
        SELECT payload::text AS payload, received_at
        FROM agent_metric_reports
        WHERE host_id=$1::uuid
          AND ($2::timestamptz IS NULL OR collected_at >= $2)
          AND ($3::timestamptz IS NULL OR collected_at <= $3)
        ORDER BY collected_at DESC, report_id DESC
        LIMIT $4
        "#,
    )
    .bind(host_id)
    .bind(from)
    .bind(to)
    .bind(limit)
    .fetch_all(pool)
    .await?;
    let mut points = rows
        .into_iter()
        .map(|row| {
            Ok(StoredHistoryPoint {
                report: serde_json::from_str(&row.try_get::<String, _>("payload")?)?,
                received_at: row.try_get("received_at")?,
            })
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    points.reverse();
    Ok(points)
}

pub async fn prune_monitoring_history(pool: &DbPool, retention_days: i64) -> anyhow::Result<u64> {
    Ok(query(
        "DELETE FROM agent_metric_reports WHERE received_at < NOW() - ($1 * INTERVAL '1 day')",
    )
    .bind(retention_days.clamp(1, 3650))
    .execute(pool)
    .await?
    .rows_affected())
}

fn host_select(suffix: &str) -> String {
    format!(
        r#"
        SELECT host_id::text AS host_id,name,os,os_version,kernel_version,arch,agent_version,
               capabilities::text AS capabilities,registered_at,last_seen_at,
               latest_collected_at,latest_interval_seconds,latest_report::text AS latest_report
        FROM monitored_hosts {suffix}
        "#
    )
}

fn stored_host_from_row(row: sqlx_postgres::PgRow) -> anyhow::Result<StoredHost> {
    let capabilities = serde_json::from_str(&row.try_get::<String, _>("capabilities")?)?;
    let latest = row
        .try_get::<Option<String>, _>("latest_report")?
        .map(|payload| serde_json::from_str(&payload))
        .transpose()?;
    Ok(StoredHost {
        identity: HostIdentity {
            id: row.try_get("host_id")?,
            name: row.try_get("name")?,
            os: row.try_get("os")?,
            os_version: row.try_get("os_version")?,
            kernel_version: row.try_get("kernel_version")?,
            arch: row.try_get("arch")?,
            agent_version: row.try_get("agent_version")?,
        },
        capabilities,
        registered_at: row.try_get("registered_at")?,
        last_seen_at: row.try_get("last_seen_at")?,
        latest_collected_at: row.try_get("latest_collected_at")?,
        latest_interval_seconds: row.try_get("latest_interval_seconds")?,
        latest,
    })
}

fn canonical_uuid(value: &str) -> anyhow::Result<String> {
    Ok(uuid::Uuid::parse_str(value)?.to_string())
}
