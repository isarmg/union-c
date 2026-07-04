//! 运行配置读写：从 PostgreSQL 的键值表加载和持久化 `Settings`。
//!
//! # 键值配置存储模式（Key-Value Config Store）
//!
//! 这个模块实现了一种常见的配置持久化模式：
//! 将整个配置结构（`Settings`）序列化为 JSON 字符串，
//! 存储在数据库的 `settings` 表中，以一个固定键（`APP_SETTINGS_KEY`）标识。
//!
//! 优点：
//! - 不需要为每个配置项单独建列，扩展新配置项无需改数据库 schema
//! - 配置结构在 Rust 类型系统中完整定义，编译器保证类型安全
//! - 支持嵌套结构和数组（如 `sunshine.hosts[]`）
//!
//! 缺点：
//! - 无法用 SQL 直接查询/过滤某个具体的配置值
//! - 整个配置必须一次性读写，不能只更新某一个字段
//!
//! # 配置的生命周期
//!
//! 1. 首次启动：从启动参数/环境变量读取 `bootstrap` 配置，写入数据库
//! 2. 之后每次启动：从数据库读取配置（覆盖文件/环境变量中的对应值）
//! 3. 用户通过 API 修改主机列表等配置：立即写回数据库（`save_app_settings`）

use sqlx_core::{query::query, row::Row};
use sqlx_postgres::PgConnection;

use crate::app_config::Settings;

use super::DbPool;

/// 存储运行配置的数据库键名。
/// 使用 "app." 前缀是命名空间约定，避免和其他设置键冲突。
pub(super) const APP_SETTINGS_KEY: &str = "app.runtime_settings";

/// 从 PostgreSQL 加载运行配置；首次运行时从启动配置种子化写入。
///
/// # 首次运行 vs 后续运行
///
/// - 首次运行（数据库中没有此键）：把 `bootstrap`（启动时读取的配置）写入数据库并返回
/// - 后续运行（数据库中已有此键）：从数据库读取，用数据库中的值覆盖 `bootstrap`
///
/// 注意：`settings.database` 始终使用 `bootstrap.database`，
/// 不从数据库读取，因为数据库连接配置本身无法从数据库中获取（先有鸡先有蛋问题）。
///
pub async fn load_or_seed_app_settings(
    pool: &DbPool,
    bootstrap: &Settings,
) -> anyhow::Result<Settings> {
    if let Some(stored) = get_setting(pool, APP_SETTINGS_KEY).await? {
        if !crate::secrets::is_encrypted(&stored) {
            anyhow::bail!("unencrypted app runtime settings are not supported");
        }
        let raw = crate::secrets::decrypt(&stored)?;

        // 将 JSON 字符串反序列化为 Settings 结构
        // `serde_json::from_str` 是 Rust 的 JSON 反序列化，
        // Settings 结构体需要派生 `#[derive(Deserialize)]`
        let mut settings: Settings = serde_json::from_str(&raw)
            .map_err(|err| anyhow::anyhow!("invalid app runtime settings in PostgreSQL: {err}"))?;

        // 数据库连接配置始终来自启动参数，不从数据库读取
        settings.database = bootstrap.database.clone();
        return Ok(settings);
    }

    // 首次运行：把 bootstrap 配置序列化为 JSON 并写入数据库
    // `serde_json::to_string_pretty` 生成格式化（有缩进）的 JSON，便于人工查看和调试
    let value = serde_json::to_string_pretty(bootstrap)
        .map_err(|err| anyhow::anyhow!("failed to serialize app runtime settings: {err}"))?;
    set_setting(pool, APP_SETTINGS_KEY, &crate::secrets::encrypt(&value)?).await?;
    Ok(bootstrap.clone())
}

/// 原子保存运行配置并注册主机地址，避免一项成功、另一项失败。
pub async fn save_app_settings_and_register_host(
    pool: &DbPool,
    settings: &Settings,
    kind: &str,
    id: &str,
    address: &str,
) -> anyhow::Result<()> {
    let value = encrypted_app_settings(settings)?;
    let mut tx = pool.begin().await?;
    upsert_setting(&mut tx, APP_SETTINGS_KEY, &value).await?;
    query(
        "INSERT INTO managed_host_addresses(kind,host_id,address) VALUES($1,$2,$3) \
         ON CONFLICT(kind,host_id) DO UPDATE SET address=EXCLUDED.address,updated_at=NOW()",
    )
    .bind(kind)
    .bind(id)
    .bind(address)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}

/// 原子保存运行配置并移除主机地址。
pub async fn save_app_settings_and_unregister_host(
    pool: &DbPool,
    settings: &Settings,
    kind: &str,
    id: &str,
) -> anyhow::Result<()> {
    let value = encrypted_app_settings(settings)?;
    let mut tx = pool.begin().await?;
    upsert_setting(&mut tx, APP_SETTINGS_KEY, &value).await?;
    query("DELETE FROM managed_host_addresses WHERE kind=$1 AND host_id=$2")
        .bind(kind)
        .bind(id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

fn encrypted_app_settings(settings: &Settings) -> anyhow::Result<String> {
    let value = serde_json::to_string_pretty(settings)
        .map_err(|err| anyhow::anyhow!("failed to serialize app settings: {err}"))?;
    crate::secrets::encrypt(&value)
}

/// 读取一个键值设置，返回 `Option<String>`（键不存在时返回 None）。
///
/// `settings` 表是通用的键值存储：
/// - `setting_key`：键（字符串，如 "app.runtime_settings"）
/// - `value`：值（字符串，通常是 JSON）
///
/// 这个函数是底层读取原语，上层函数负责 JSON 解析。
pub async fn get_setting(pool: &DbPool, key: &str) -> anyhow::Result<Option<String>> {
    let row = query(
        r#"
        SELECT value
        FROM settings
        WHERE setting_key = $1
        "#,
    )
    .bind(key)
    .fetch_optional(pool) // 找不到时返回 None，不报错
    .await?;

    // `row.map(...).transpose()` 将 `Option<Result<String>>` 转换为 `Result<Option<String>>`
    row.map(|row| row.try_get("value").map_err(Into::into))
        .transpose()
}

/// 写入一个键值设置（upsert：不存在则插入，已存在则更新）。
///
/// # UPSERT 语法
///
/// `ON CONFLICT (setting_key) DO UPDATE SET ...` 的含义：
/// - 如果 `setting_key` 不存在：执行 INSERT
/// - 如果 `setting_key` 已存在（唯一约束冲突）：执行 UPDATE
///
/// `EXCLUDED.value` 引用本次试图插入的新值，
/// `updated_at = NOW()` 每次写入都更新时间戳（便于追踪最后修改时间）。
pub async fn set_setting(pool: &DbPool, key: &str, value: &str) -> anyhow::Result<()> {
    let mut connection = pool.acquire().await?;
    upsert_setting(&mut connection, key, value).await
}

async fn upsert_setting(
    connection: &mut PgConnection,
    key: &str,
    value: &str,
) -> anyhow::Result<()> {
    query(
        r#"
        INSERT INTO settings (setting_key, value, updated_at)
        VALUES ($1, $2, NOW())
        ON CONFLICT (setting_key) DO UPDATE SET
            value      = EXCLUDED.value,
            updated_at = NOW()
        "#,
    )
    .bind(key)
    .bind(value)
    .execute(connection)
    .await?;

    Ok(())
}
