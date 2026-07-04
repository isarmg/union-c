//! 分类、标签候选项及分类标签关系持久化。

use super::*;

// ─── 标签与分类候选项 ─────────────────────────────────────────────────────────

/// 读取指定类型的标签/分类候选项（`kind` 为 "tag" 或 "category"）。
///
/// 候选项是前端下拉选单的数据来源。用户也可以在编辑文章时创建新的候选项。
pub async fn list_blog_taxonomy(pool: &DbPool, kind: &str) -> anyhow::Result<Vec<String>> {
    let rows = query(
        r#"
        SELECT name
        FROM blog_taxonomy
        WHERE kind = $1
        ORDER BY name ASC
        "#,
    )
    .bind(kind)
    .fetch_all(pool)
    .await?;

    // 将每行的 "name" 列提取为 String，遇到错误提前返回
    rows.into_iter()
        .map(|row| row.try_get("name").map_err(Into::into))
        .collect()
}

/// 新增标签/分类候选项，返回是否实际插入了新记录（已存在则返回 false）。
pub async fn insert_blog_taxonomy(pool: &DbPool, kind: &str, name: &str) -> anyhow::Result<bool> {
    let result = query(
        r#"
        INSERT INTO blog_taxonomy (kind, name)
        VALUES ($1, $2)
        ON CONFLICT DO NOTHING
        "#,
    )
    .bind(kind)
    .bind(name)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

/// 删除标签/分类候选项，返回是否实际删除了记录。
pub async fn delete_blog_taxonomy(pool: &DbPool, kind: &str, name: &str) -> anyhow::Result<bool> {
    let result = query(
        r#"
        DELETE FROM blog_taxonomy
        WHERE kind = $1 AND name = $2
        "#,
    )
    .bind(kind)
    .bind(name)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

/// 读取所有分类下的标签候选关系，返回 `(category, tag)` 元组列表。
///
/// 这个数据用于前端"分类联动标签"功能：
/// 用户选择某个分类后，标签下拉选项自动缩小到该分类常用的标签集合。
pub async fn list_blog_category_tags(pool: &DbPool) -> anyhow::Result<Vec<(String, String)>> {
    let rows = query(
        r#"
        SELECT category, tag
        FROM blog_category_tags
        ORDER BY category ASC, tag ASC
        "#,
    )
    .fetch_all(pool)
    .await?;

    rows.into_iter()
        .map(|row| Ok((row.try_get("category")?, row.try_get("tag")?)))
        .collect()
}

/// 把一个标签加入指定分类的可选集合。
pub async fn insert_blog_category_tag(
    pool: &DbPool,
    category: &str,
    tag: &str,
) -> anyhow::Result<bool> {
    let result = query(
        r#"
        INSERT INTO blog_category_tags (category, tag)
        VALUES ($1, $2)
        ON CONFLICT DO NOTHING
        "#,
    )
    .bind(category)
    .bind(tag)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

/// 从指定分类的可选集合里删除一个标签。
pub async fn delete_blog_category_tag(
    pool: &DbPool,
    category: &str,
    tag: &str,
) -> anyhow::Result<bool> {
    let result = query(
        r#"
        DELETE FROM blog_category_tags
        WHERE category = $1 AND tag = $2
        "#,
    )
    .bind(category)
    .bind(tag)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

/// 删除某个标签在所有分类下的候选关系（删除标签时调用）。
///
/// 返回被删除的关联记录数。
pub async fn delete_blog_category_tag_everywhere(pool: &DbPool, tag: &str) -> anyhow::Result<u64> {
    let result = query("DELETE FROM blog_category_tags WHERE tag = $1")
        .bind(tag)
        .execute(pool)
        .await?;
    Ok(result.rows_affected())
}

/// 删除某个分类下全部标签候选关系（删除分类时调用）。
pub async fn delete_blog_category_tags_for_category(
    pool: &DbPool,
    category: &str,
) -> anyhow::Result<u64> {
    let result = query("DELETE FROM blog_category_tags WHERE category = $1")
        .bind(category)
        .execute(pool)
        .await?;
    Ok(result.rows_affected())
}

/// 重命名所有分类下的某个标签（标签重命名时调用）。
///
/// # 为什么不直接 UPDATE？
///
/// `blog_category_tags` 表的主键可能是 `(category, tag)` 复合唯一约束，
/// 直接 UPDATE tag 可能与已存在的记录冲突（如果新名称已经存在于某分类）。
/// 所以采用"先 INSERT 新名称，再 DELETE 旧名称"的两步策略：
/// 1. INSERT ... SELECT：把旧名称所有行以新名称复制一份（跳过已存在的）
/// 2. DELETE：删除旧名称的所有行
///
/// 整个过程在事务中进行，保证原子性。
pub async fn rename_blog_category_tag_everywhere(
    pool: &DbPool,
    from: &str,
    to: &str,
) -> anyhow::Result<u64> {
    let mut tx = pool.begin().await?;
    // INSERT ... SELECT 从现有记录批量复制（把 tag = from 改名为 to）
    query(
        r#"
        INSERT INTO blog_category_tags (category, tag)
        SELECT category, $1 FROM blog_category_tags WHERE tag = $2
        ON CONFLICT DO NOTHING
        "#,
    )
    .bind(to)
    .bind(from)
    .execute(&mut *tx)
    .await?;
    let deleted = query("DELETE FROM blog_category_tags WHERE tag = $1")
        .bind(from)
        .execute(&mut *tx)
        .await?
        .rows_affected();
    tx.commit().await?;
    Ok(deleted)
}

/// 重命名单个分类内的标签候选项（只影响特定分类，其他分类的同名标签不变）。
pub async fn rename_blog_category_tag(
    pool: &DbPool,
    category: &str,
    from: &str,
    to: &str,
) -> anyhow::Result<u64> {
    let mut tx = pool.begin().await?;
    // 先插入新名称
    query(
        r#"
        INSERT INTO blog_category_tags (category, tag)
        VALUES ($1, $2)
        ON CONFLICT DO NOTHING
        "#,
    )
    .bind(category)
    .bind(to)
    .execute(&mut *tx)
    .await?;
    // 再删除旧名称
    let deleted = query("DELETE FROM blog_category_tags WHERE category = $1 AND tag = $2")
        .bind(category)
        .bind(from)
        .execute(&mut *tx)
        .await?
        .rows_affected();
    tx.commit().await?;
    Ok(deleted)
}

/// 重命名分类标签关系中的分类名（分类重命名时调用，同样采用"先插后删"策略）。
pub async fn rename_blog_category_tags_category(
    pool: &DbPool,
    from: &str,
    to: &str,
) -> anyhow::Result<u64> {
    let mut tx = pool.begin().await?;
    // 将旧分类名下的所有标签关系以新分类名复制
    query(
        r#"
        INSERT INTO blog_category_tags (category, tag)
        SELECT $1, tag FROM blog_category_tags WHERE category = $2
        ON CONFLICT DO NOTHING
        "#,
    )
    .bind(to)
    .bind(from)
    .execute(&mut *tx)
    .await?;
    let deleted = query("DELETE FROM blog_category_tags WHERE category = $1")
        .bind(from)
        .execute(&mut *tx)
        .await?
        .rows_affected();
    tx.commit().await?;
    Ok(deleted)
}

/// 在单个事务中批量写入 blog_taxonomy 条目，忽略已存在的记录。
///
/// 用于批量导入时（如从文件系统扫描所有标签），比逐条插入效率更高
/// （减少事务提交次数，通常快 10-100 倍）。
pub async fn batch_insert_taxonomy(
    pool: &DbPool,
    entries: &[(String, String)],
) -> anyhow::Result<()> {
    if entries.is_empty() {
        return Ok(()); // 无需操作时提前返回，避免开启空事务
    }
    let mut tx = pool.begin().await?;
    for (kind, name) in entries {
        query("INSERT INTO blog_taxonomy (kind, name) VALUES ($1, $2) ON CONFLICT DO NOTHING")
            .bind(kind)
            .bind(name)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(())
}

/// 在单个事务中批量写入 blog_category_tags 条目，忽略已存在的记录。
pub async fn batch_insert_category_tags(
    pool: &DbPool,
    entries: &[(String, String)],
) -> anyhow::Result<()> {
    if entries.is_empty() {
        return Ok(());
    }
    let mut tx = pool.begin().await?;
    for (category, tag) in entries {
        query(
            "INSERT INTO blog_category_tags (category, tag) VALUES ($1, $2) ON CONFLICT DO NOTHING",
        )
        .bind(category)
        .bind(tag)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}
