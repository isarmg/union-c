//! 服务账号和路径权限持久化。

use super::*;

// ─── 服务账号类型 ─────────────────────────────────────────────────────────────

/// 写入服务账号时使用的路径权限条目。
#[derive(Debug, Clone)]
pub struct ServicePermissionInput {
    pub resource_path: String, // 受权限保护的路径（如 "/public" 或 "/private"）
    pub permission: String,    // 权限级别（如 "read" 或 "readwrite"）
}

/// 写入数据库时使用的服务账号数据结构。
#[derive(Debug, Clone)]
pub struct ServiceAccountInput {
    pub account_key: String,             // 账号的唯一标识键（用于更新时的匹配）
    pub username: Option<String>,        // 具名账号的用户名（匿名账号为 None）
    pub password_secret: Option<String>, // 待加密保存的密码；数据库中只存密文
    pub is_anonymous: bool,              // true = 匿名访问账号（无需认证）
    pub is_management: bool,             // true = 管理账号（拥有更高权限）
    pub permissions: Vec<ServicePermissionInput>, // 该账号的路径权限列表
}

/// 从数据库读取的路径权限记录。
#[derive(Debug, Clone)]
pub struct ServicePermissionRecord {
    pub resource_path: String,
    pub permission: String,
}

/// 从数据库读取的服务账号完整记录（含权限列表）。
#[derive(Debug, Clone)]
pub struct ServiceAccountRecord {
    pub username: Option<String>,
    pub password_secret: Option<String>,
    pub is_anonymous: bool,
    pub is_management: bool,
    pub permissions: Vec<ServicePermissionRecord>,
}

// ─── 服务账号 ─────────────────────────────────────────────────────────────────

/// 读取某个服务的全部启用账号及其路径权限。
///
/// # SQL JOIN 与去重处理
///
/// 因为使用了 LEFT JOIN，一个账号如果有多条权限记录，
/// 会在结果中出现多行（每条权限一行）。
/// Rust 侧通过 `last_id` 跟踪"当前账号 ID"，
/// 遇到新 ID 时创建新的 `ServiceAccountRecord`，
/// 遇到相同 ID 的行则把权限追加到最后一个账号的权限列表中。
/// 这种模式叫"结果集折叠"（Result Set Folding），比 N+1 查询高效。
///
/// # ORDER BY 的作用
///
/// 特定的排序确保同一账号的所有行是连续的：
/// - 先按账号 ID 排序（`sa.id ASC`）：同一账号的所有行相邻
/// - `sa.is_management ASC`：普通账号排在管理账号前面
/// - `sa.is_anonymous DESC`：匿名账号排在前面（DESC 使 true 排前面）
pub async fn service_accounts(
    pool: &DbPool,
    service_name: &str,
) -> anyhow::Result<Vec<ServiceAccountRecord>> {
    let rows = query(
        r#"
        SELECT
            sa.id,
            sa.username,
            sa.password_secret,
            sa.is_anonymous,
            sa.is_management,
            sap.resource_path,
            sap.permission
        FROM service_accounts sa
        LEFT JOIN service_account_permissions sap ON sap.account_id = sa.id
        WHERE sa.service_name = $1 AND sa.enabled = TRUE
        ORDER BY sa.is_management ASC, sa.is_anonymous DESC, sa.username ASC, sa.id ASC, sap.id ASC
        "#,
    )
    .bind(service_name)
    .fetch_all(pool)
    .await?;

    let mut accounts: Vec<ServiceAccountRecord> = Vec::new();
    let mut last_id: Option<i64> = None;

    for row in rows {
        let id: i64 = row.try_get("id")?;
        if last_id != Some(id) {
            // 遇到新的账号 ID：创建新的账号记录并追加到列表
            last_id = Some(id);
            accounts.push(ServiceAccountRecord {
                username: row.try_get("username")?,
                password_secret: row
                    .try_get::<Option<String>, _>("password_secret")?
                    .map(|value| crate::secrets::decrypt(&value))
                    .transpose()?,
                is_anonymous: row.try_get("is_anonymous")?,
                is_management: row.try_get("is_management")?,
                permissions: Vec::new(), // 权限列表从空开始，后续同 ID 的行会追加进来
            });
        }
        // 如果该行有权限记录（LEFT JOIN 的右侧不为 NULL）
        let resource_path: Option<String> = row.try_get("resource_path")?;
        if let (Some(resource_path), Some(account)) = (resource_path, accounts.last_mut()) {
            // `accounts.last_mut()` 获取当前最后一个账号的可变引用，即刚刚处理的账号
            account.permissions.push(ServicePermissionRecord {
                resource_path,
                permission: row.try_get("permission")?,
            });
        }
    }

    Ok(accounts)
}

/// 统计某个服务已有账号数量（用于判断是否需要初始化默认账号）。
pub async fn count_service_accounts(pool: &DbPool, service_name: &str) -> anyhow::Result<i64> {
    let row = query(
        r#"
        SELECT COUNT(*) AS account_count
        FROM service_accounts
        WHERE service_name = $1
        "#,
    )
    .bind(service_name)
    .fetch_one(pool)
    .await?;

    Ok(row.try_get("account_count")?)
}

/// 完整替换某个服务的账号列表（事务保证原子性）。
///
/// # 为什么是"全量替换"而不是"增量更新"？
///
/// ram 的账号配置通常一次性提交全量数据（前端发送完整的账号列表）。
/// 全量替换比增量更新更简单，不需要计算差量（哪些需要新增、哪些需要删除）。
/// 事务保证：要么新配置完整生效，要么失败时保留旧配置，不会出现"半更新"状态。
///
/// # 删除顺序
///
/// 先删权限记录，再删账号记录（因为权限表外键依赖账号表）。
/// 如果先删账号，数据库外键约束可能会报错（或级联删除权限，取决于数据库配置）。
pub async fn replace_service_accounts(
    pool: &DbPool,
    service_name: &str,
    accounts: &[ServiceAccountInput],
) -> anyhow::Result<()> {
    let mut tx = pool.begin().await?;

    // 步骤 1：删除该服务所有账号的权限记录
    // 子查询 `SELECT id FROM service_accounts WHERE service_name = $1` 找出该服务的账号 ID
    query(
        r#"
        DELETE FROM service_account_permissions
        WHERE account_id IN (
            SELECT id FROM service_accounts WHERE service_name = $1
        )
        "#,
    )
    .bind(service_name)
    .execute(&mut *tx)
    .await?;

    // 步骤 2：删除该服务的所有账号记录
    query("DELETE FROM service_accounts WHERE service_name = $1")
        .bind(service_name)
        .execute(&mut *tx)
        .await?;

    // 步骤 3：重新插入新的账号和权限
    for account in accounts {
        // `RETURNING id` 让 PostgreSQL 在 INSERT 后返回新生成的 id
        // 需要用 `fetch_one` 而不是 `execute`，因为要读取返回的 id
        let row = query(
            r#"
            INSERT INTO service_accounts (
                service_name, account_key, username, password_secret,
                is_anonymous, is_management, enabled
            )
            VALUES ($1, $2, $3, $4, $5, $6, TRUE)
            RETURNING id
            "#,
        )
        .bind(service_name)
        .bind(&account.account_key)
        .bind(&account.username)
        .bind(
            account
                .password_secret
                .as_deref()
                .map(crate::secrets::encrypt)
                .transpose()?,
        )
        .bind(account.is_anonymous)
        .bind(account.is_management)
        .fetch_one(&mut *tx)
        .await?;

        let account_id: i64 = row.try_get("id")?;

        // 为每条权限记录插入关联行（使用刚刚获得的 account_id）
        for permission in &account.permissions {
            query(
                r#"
                INSERT INTO service_account_permissions (account_id, resource_path, permission)
                VALUES ($1, $2, $3)
                "#,
            )
            .bind(account_id)
            .bind(&permission.resource_path)
            .bind(&permission.permission)
            .execute(&mut *tx)
            .await?;
        }
    }

    tx.commit().await?;
    Ok(())
}
