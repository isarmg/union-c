//! 数据库访问层入口。
//!
//! 各领域的 SQL 函数按功能拆分到子模块，通过 `pub use` 统一导出，
//! 调用方仍然使用 `database::function_name()` 形式，无需感知内部分层。
//!
//! 子模块职责：
//! - `settings`  — 键值配置读写、启动设置加载与迁移
//! - `audit`     — 审计日志、服务事件、后台任务、RAM 服务账号
//! - `blog`      — 博客文章、分类、标签
//! - `ram_instances` — 远程 RAM 实例

mod audit;
mod blog;
mod ram_instances;
mod settings;

pub use audit::*;
pub use blog::*;
pub use ram_instances::*;
pub use settings::*;

use sha2::{Digest, Sha256};
use sqlx_core::{executor::Executor, query::query, raw_sql::raw_sql, row::Row};
use sqlx_postgres::{PgConnection, PgPool, PgPoolOptions};
use std::time::Duration;
use url::Url;

use crate::app_config::Settings;

/// 项目里统一使用的数据库连接池类型。
pub type DbPool = PgPool;

/// 构造一个不会在启动阶段发起网络连接的占位池。
pub fn disconnected_pool() -> anyhow::Result<DbPool> {
    Ok(PgPoolOptions::new()
        .max_connections(1)
        .acquire_timeout(Duration::from_secs(2))
        .connect_lazy("postgresql://union:unused@127.0.0.1:1/union_unconfigured")?)
}

/// 连接 PostgreSQL，数据库不存在时自动创建。
pub async fn connect(settings: &Settings) -> anyhow::Result<DbPool> {
    match PgPoolOptions::new()
        .max_connections(8)
        .acquire_timeout(Duration::from_secs(10))
        .idle_timeout(Duration::from_secs(300))
        .max_lifetime(Duration::from_secs(1800))
        .connect(&settings.database.url)
        .await
    {
        Ok(pool) => Ok(pool),
        Err(err) => {
            if settings.production {
                return Err(anyhow::anyhow!(
                    "failed to connect to the pre-provisioned production PostgreSQL database: {err}"
                ));
            }
            ensure_database_exists(&settings.database.url).await?;
            PgPoolOptions::new()
                .max_connections(8)
                .acquire_timeout(Duration::from_secs(10))
                .connect(&settings.database.url)
                .await
                .map_err(|next_err| {
                    anyhow::anyhow!(
                        "failed to connect to PostgreSQL after database initialization: {next_err}; initial error: {err}"
                    )
                })
        }
    }
}

/// 自动创建目标 PostgreSQL 数据库（如果不存在）。
async fn ensure_database_exists(database_url: &str) -> anyhow::Result<()> {
    let (server_url, database_name) = split_pg_url(database_url)?;
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect(&server_url)
        .await?;

    let row = query("SELECT EXISTS(SELECT FROM pg_database WHERE datname = $1) AS exists")
        .bind(&database_name)
        .fetch_one(&pool)
        .await?;
    let exists: bool = row.try_get("exists")?;

    if !exists {
        let escaped = database_name.replace('"', "\"\"");
        query(&format!("CREATE DATABASE \"{escaped}\""))
            .execute(&pool)
            .await?;
    }
    Ok(())
}

/// 把完整 PostgreSQL URL 拆成"服务器 URL（指向默认库 postgres）"和"数据库名"。
fn split_pg_url(database_url: &str) -> anyhow::Result<(String, String)> {
    let mut url = Url::parse(database_url)?;
    let database_name = url
        .path_segments()
        .and_then(|mut segments| segments.next().map(ToOwned::to_owned))
        .filter(|segment| !segment.trim().is_empty())
        .ok_or_else(|| anyhow::anyhow!("PostgreSQL URL must include a database name"))?;
    url.set_path("/postgres");
    Ok((url.to_string(), database_name))
}

/// 创建所有表并写入基础服务行。
pub async fn migrate(pool: &DbPool) -> anyhow::Result<()> {
    // PostgreSQL advisory lock 是一个由应用自行约定编号的互斥锁。多实例同时启动时，
    // 后到的实例会等待，避免两个进程重复判断并执行同一个版本。
    // 锁属于当前数据库连接，所以 lock_connection 必须一直保持到迁移结束。
    let mut lock_connection = pool.acquire().await?;
    query("SELECT pg_advisory_lock($1)")
        .bind(718_204_202_i64)
        .execute(&mut *lock_connection)
        .await?;
    // 所有待执行版本共用一个事务：任意 SQL 失败都会回滚，不留下“只建了一半”的结构。
    query("BEGIN").execute(&mut *lock_connection).await?;
    let result: anyhow::Result<()> = match migrate_inner(&mut lock_connection).await {
        Ok(()) => query("COMMIT")
            .execute(&mut *lock_connection)
            .await
            .map(|_| ())
            .map_err(Into::into),
        Err(err) => {
            let _ = query("ROLLBACK").execute(&mut *lock_connection).await;
            Err(err)
        }
    };
    let unlock_result = query("SELECT pg_advisory_unlock($1)")
        .bind(718_204_202_i64)
        .execute(&mut *lock_connection)
        .await;
    result?;
    unlock_result?;
    Ok(())
}

async fn migrate_inner(connection: &mut PgConnection) -> anyhow::Result<()> {
    // schema_migrations 是迁移器自己的账本，不属于业务数据。它记录已执行版本，
    // 并通过校验和验证已登记 SQL 的内容完整性。
    query(
        r#"
        CREATE TABLE IF NOT EXISTS schema_migrations (
            version     BIGINT PRIMARY KEY,
            description TEXT NOT NULL,
            applied_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
        )
        "#,
    )
    .execute(&mut *connection)
    .await?;
    query("ALTER TABLE schema_migrations ADD COLUMN IF NOT EXISTS checksum TEXT")
        .execute(&mut *connection)
        .await?;

    for migration in MIGRATIONS {
        // 校验和覆盖 SQL 文件的全部字节，包括空格和注释。因此历史迁移一旦在任何
        // 环境执行过便不可编辑；数据库变化应通过新增更高版本来表达。
        let checksum = migration_checksum(migration.sql);
        let applied = query("SELECT checksum FROM schema_migrations WHERE version = $1")
            .bind(migration.version)
            .fetch_optional(&mut *connection)
            .await?;
        if let Some(row) = applied {
            let stored: Option<String> = row.try_get("checksum")?;
            if let Some(stored) = stored {
                if stored != checksum {
                    anyhow::bail!(
                        "migration {} checksum mismatch; applied migrations must not be edited",
                        migration.version
                    );
                }
            } else {
                // checksum 为空时补写当前 SQL 的校验和，后续启动即可执行完整性检查。
                query("UPDATE schema_migrations SET checksum = $2 WHERE version = $1")
                    .bind(migration.version)
                    .bind(&checksum)
                    .execute(&mut *connection)
                    .await?;
            }
            continue;
        }

        // raw_sql 允许一个迁移文件包含多条由分号分隔的 DDL/DML 语句。
        (&mut *connection).execute(raw_sql(migration.sql)).await?;
        query("INSERT INTO schema_migrations (version, description, checksum) VALUES ($1, $2, $3)")
            .bind(migration.version)
            .bind(migration.description)
            .bind(checksum)
            .execute(&mut *connection)
            .await?;
    }
    Ok(())
}

struct Migration {
    version: i64,
    description: &'static str,
    sql: &'static str,
}

// 目录不会被运行时自动扫描。include_str! 在编译期把 SQL 放进 union 二进制，部署时
// 不需要复制 migrations 目录；新增 SQL 文件后也必须在此数组登记版本和说明。
const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        description: "production baseline schema",
        sql: include_str!("../../migrations/0001_baseline.sql"),
    },
    Migration {
        version: 2,
        description: "storage integrity and cleanup",
        sql: include_str!("../../migrations/0002_storage_integrity.sql"),
    },
    Migration {
        version: 3,
        description: "data shape constraints",
        sql: include_str!("../../migrations/0003_data_shape_constraints.sql"),
    },
    Migration {
        version: 4,
        description: "ram instance TLS column compatibility",
        sql: include_str!("../../migrations/0004_ram_instance_tls_columns.sql"),
    },
];

fn migration_checksum(sql: &str) -> String {
    Sha256::digest(sql.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// 就绪探针使用的最小数据库往返。
pub async fn ping(pool: &DbPool) -> anyhow::Result<()> {
    query("SELECT 1").execute(pool).await?;
    Ok(())
}
