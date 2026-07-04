//! 博客文章及文章标签持久化。

use super::*;

// ─── 博客文章 ─────────────────────────────────────────────────────────────────

/// 列出全部博客文章，含标签数组。
///
/// # SQL 说明
///
/// `COALESCE(column, default)` 在列为 NULL 时返回默认值，避免 Rust 侧处理 NULL。
///
/// `to_char(updated_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"')` 的含义：
/// - `AT TIME ZONE 'UTC'`：将时间戳转换为 UTC 时区
/// - `to_char(..., 'YYYY-MM-DD"T"HH24:MI:SS"Z"')`：格式化为 ISO 8601 字符串（如 "2024-01-01T12:00:00Z"）
/// - 引号内的 `T` 和 `Z` 是字面字符，不是格式占位符
///
/// # 标签的 N+1 问题优化
///
/// 没有用子查询或 LEFT JOIN 读取标签（会产生多行），
/// 而是先查所有文章，再批量查所有标签（2次查询而非 N+1 次），
/// 最后在 Rust 侧用 `BTreeMap` 关联。
pub async fn list_blog_posts(pool: &DbPool) -> anyhow::Result<Vec<BlogPostRecord>> {
    let rows = query(
        r#"
        SELECT
            id,
            relative_path,
            COALESCE(extension, 'md') AS extension,
            title,
            COALESCE(description, '') AS description,
            ''::TEXT AS content,
            draft,
            featured,
            pub_date::TEXT AS pub_date,
            updated_date::TEXT AS updated_date,
            author,
            category,
            series,
            hero_image,
            to_char(updated_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"') AS updated_at
        FROM blog_posts
        ORDER BY pub_date DESC, title ASC, relative_path ASC
        "#,
    )
    .fetch_all(pool)
    .await?;

    // 批量查询所有文章的标签（post_id → Vec<tag>），避免对每篇文章单独查询
    let tags = blog_post_tags(pool).await?;

    // 将每一行数据库记录与对应的标签列表合并
    rows.into_iter()
        .map(|row| blog_post_record_from_row(row, &tags))
        .collect() // `.collect()` 将迭代器收集为 `Vec<BlogPostRecord>`（或在遇到 Err 时提前返回）
}

pub async fn list_blog_posts_page(
    pool: &DbPool,
    limit: i64,
    offset: i64,
) -> anyhow::Result<Vec<BlogPostRecord>> {
    let rows = query(
        r#"
        SELECT
            id, relative_path, COALESCE(extension, 'md') AS extension,
            title, COALESCE(description, '') AS description,
            ''::TEXT AS content, draft, featured,
            pub_date::TEXT AS pub_date, updated_date::TEXT AS updated_date,
            author, category, series, hero_image,
            to_char(updated_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"') AS updated_at
        FROM blog_posts
        ORDER BY pub_date DESC, title ASC, relative_path ASC
        LIMIT $1 OFFSET $2
        "#,
    )
    .bind(limit.clamp(1, 500))
    .bind(offset.max(0))
    .fetch_all(pool)
    .await?;
    let tags = blog_post_tags_for_ids(
        pool,
        &rows
            .iter()
            .filter_map(|row| row.try_get::<String, _>("id").ok())
            .collect::<Vec<_>>(),
    )
    .await?;
    rows.into_iter()
        .map(|row| blog_post_record_from_row(row, &tags))
        .collect()
}

/// 列出全部文章并包含正文，仅供导出和批量改写使用。
pub async fn list_blog_posts_with_content(pool: &DbPool) -> anyhow::Result<Vec<BlogPostRecord>> {
    let rows = query(
        r#"
        SELECT
            id,
            relative_path,
            COALESCE(extension, 'md') AS extension,
            title,
            COALESCE(description, '') AS description,
            COALESCE(content, '') AS content,
            draft,
            featured,
            pub_date::TEXT AS pub_date,
            updated_date::TEXT AS updated_date,
            author,
            category,
            series,
            hero_image,
            to_char(updated_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"') AS updated_at
        FROM blog_posts
        ORDER BY pub_date DESC, title ASC, relative_path ASC
        "#,
    )
    .fetch_all(pool)
    .await?;
    let tags = blog_post_tags(pool).await?;
    rows.into_iter()
        .map(|row| blog_post_record_from_row(row, &tags))
        .collect()
}

/// 按相对路径读取单篇博客文章（含标签）。
pub async fn blog_post_by_path(
    pool: &DbPool,
    relative_path: &str,
) -> anyhow::Result<Option<BlogPostRecord>> {
    let row = query(
        r#"
        SELECT
            id,
            relative_path,
            COALESCE(extension, 'md') AS extension,
            title,
            COALESCE(description, '') AS description,
            COALESCE(content, '') AS content,
            draft,
            featured,
            pub_date::TEXT AS pub_date,
            updated_date::TEXT AS updated_date,
            author,
            category,
            series,
            hero_image,
            to_char(updated_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"') AS updated_at
        FROM blog_posts
        WHERE relative_path = $1
        "#,
    )
    .bind(relative_path)
    .fetch_optional(pool)
    .await?;

    // `let Some(row) else { return Ok(None) }` — 如果没找到文章，直接返回 None
    let Some(row) = row else {
        return Ok(None);
    };

    // 单篇文章只需要查询该文章的标签，不需要全量查询
    let id: String = row.try_get("id")?;
    let mut tags = BTreeMap::new();
    tags.insert(id.clone(), tags_for_blog_post(pool, &id).await?);
    Ok(Some(blog_post_record_from_row(row, &tags)?))
}

/// 新建或更新博客文章及其标签（事务保证原子性）。
///
/// # 事务的必要性
///
/// 这里在一个事务中执行多个操作：
/// 1. UPSERT blog_posts（新建或更新文章主体）
/// 2. DELETE 文章的旧标签
/// 3. INSERT 文章的新标签
///
/// 如果第 2 步成功但第 3 步失败（如数据库崩溃），
/// 没有事务保护会导致文章没有标签（数据不一致）。
/// 事务保证：要么三步都成功，要么全部回滚，不会留下中间状态。
///
/// # UPSERT 语法（ON CONFLICT DO UPDATE）
///
/// PostgreSQL 的 `INSERT ... ON CONFLICT (id) DO UPDATE SET ...` 意味着：
/// - 如果 `id` 不存在：执行 INSERT（新建）
/// - 如果 `id` 已存在：执行 UPDATE（更新）
/// - `EXCLUDED` 引用"本次试图插入的值"，`EXCLUDED.title` 就是新传入的 title
/// - `updated_at = NOW()` 由数据库自动更新时间戳
pub async fn upsert_blog_post(pool: &DbPool, post: &BlogPostInput) -> anyhow::Result<()> {
    upsert_blog_post_from_path(pool, post, None).await
}

/// 保存文章，并可在同一事务中删除重命名前的旧路径记录。
pub async fn upsert_blog_post_from_path(
    pool: &DbPool,
    post: &BlogPostInput,
    original_relative_path: Option<&str>,
) -> anyhow::Result<()> {
    // `pool.begin()` 开启数据库事务，返回 `Transaction`
    let mut tx = pool.begin().await?;
    upsert_blog_post_connection(&mut tx, post, original_relative_path).await?;
    tx.commit().await?;
    Ok(())
}

pub async fn upsert_blog_posts(pool: &DbPool, posts: &[BlogPostInput]) -> anyhow::Result<()> {
    let mut tx = pool.begin().await?;
    for post in posts {
        upsert_blog_post_connection(&mut tx, post, None).await?;
    }
    tx.commit().await?;
    Ok(())
}

async fn upsert_blog_post_connection(
    connection: &mut PgConnection,
    post: &BlogPostInput,
    original_relative_path: Option<&str>,
) -> anyhow::Result<()> {
    query(
        r#"
        INSERT INTO blog_posts (
            id, relative_path, extension, title, description, content,
            draft, featured, pub_date, updated_date, author, category, series, hero_image
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9::DATE, $10::DATE, $11, $12, $13, $14)
        ON CONFLICT (id) DO UPDATE SET
            relative_path = EXCLUDED.relative_path,
            extension     = EXCLUDED.extension,
            title         = EXCLUDED.title,
            description   = EXCLUDED.description,
            content       = EXCLUDED.content,
            draft         = EXCLUDED.draft,
            featured      = EXCLUDED.featured,
            pub_date      = EXCLUDED.pub_date,
            updated_date  = EXCLUDED.updated_date,
            author        = EXCLUDED.author,
            category      = EXCLUDED.category,
            series        = EXCLUDED.series,
            hero_image    = EXCLUDED.hero_image,
            updated_at    = NOW()
        "#,
    )
    .bind(&post.id)
    .bind(&post.relative_path)
    .bind(&post.extension)
    .bind(&post.title)
    .bind(&post.description)
    .bind(&post.content)
    .bind(post.draft)
    .bind(post.featured)
    .bind(&post.pub_date)
    .bind(&post.updated_date)
    .bind(&post.author)
    .bind(&post.category)
    .bind(&post.series)
    .bind(&post.hero_image)
    .execute(&mut *connection)
    .await?;

    // 先删除文章的所有旧标签（简单粗暴，但避免了复杂的差量计算）
    query("DELETE FROM blog_post_tags WHERE post_id = $1")
        .bind(&post.id)
        .execute(&mut *connection)
        .await?;

    // 对标签列表去重、去空白，再逐个插入
    let normalized_tags = normalized_strings(&post.tags);
    for tag in &normalized_tags {
        query(
            r#"
            INSERT INTO blog_post_tags (post_id, tag)
            VALUES ($1, $2)
            ON CONFLICT DO NOTHING
            "#,
        )
        .bind(&post.id)
        .bind(tag)
        .execute(&mut *connection)
        .await?;
    }

    // 如果文章有分类，将文章的标签自动加入该分类的"常用标签"列表
    // 这样同一分类下用过的标签会自动出现在新文章的标签下拉选项中
    if let Some(category) = post.category.as_deref() {
        query(
            "INSERT INTO blog_taxonomy (kind, name) VALUES ('category', $1) ON CONFLICT DO NOTHING",
        )
        .bind(category)
        .execute(&mut *connection)
        .await?;
        for tag in &normalized_tags {
            query(
                r#"
                INSERT INTO blog_category_tags (category, tag)
                VALUES ($1, $2)
                ON CONFLICT DO NOTHING
                "#,
            )
            .bind(category)
            .bind(tag)
            .execute(&mut *connection)
            .await?;
        }
    }
    for tag in &normalized_tags {
        query("INSERT INTO blog_taxonomy (kind, name) VALUES ('tag', $1) ON CONFLICT DO NOTHING")
            .bind(tag)
            .execute(&mut *connection)
            .await?;
    }

    if let Some(original) = original_relative_path.filter(|path| *path != post.relative_path) {
        query("DELETE FROM blog_posts WHERE relative_path = $1 AND id <> $2")
            .bind(original)
            .bind(&post.id)
            .execute(&mut *connection)
            .await?;
    }

    Ok(())
}

/// 删除指定路径的博客文章（同时级联删除关联的标签记录，需要数据库外键级联设置）。
///
/// 返回 `bool`：`true` 表示找到并删除了文章，`false` 表示文章不存在。
pub async fn delete_blog_post(pool: &DbPool, relative_path: &str) -> anyhow::Result<bool> {
    let result = query("DELETE FROM blog_posts WHERE relative_path = $1")
        .bind(relative_path)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() > 0) // `rows_affected()` 返回被删除的行数
}

// ─── 私有辅助函数 ─────────────────────────────────────────────────────────────

/// 批量查询所有文章的标签，返回 `post_id → Vec<tag>` 的映射。
///
/// # 为什么用 BTreeMap 而不是 HashMap？
///
/// `BTreeMap` 按键排序，`HashMap` 无序。这里用 `BTreeMap` 是为了保证
/// 测试可预测性和调试时的输出稳定性（每次遍历顺序相同）。
/// 性能差异在这个规模（通常几百篇文章）下可以忽略不计。
///
/// # 查询策略
///
/// 一次性查询所有文章的所有标签，在 Rust 侧按 `post_id` 分组，
/// 比为每篇文章单独查询（N 次数据库往返）效率高得多。
async fn blog_post_tags(pool: &DbPool) -> anyhow::Result<BTreeMap<String, Vec<String>>> {
    let rows = query(
        r#"
        SELECT post_id, tag
        FROM blog_post_tags
        ORDER BY post_id ASC, tag ASC
        "#,
    )
    .fetch_all(pool)
    .await?;

    let mut output = BTreeMap::<String, Vec<String>>::new();
    for row in rows {
        let post_id: String = row.try_get("post_id")?;
        let tag: String = row.try_get("tag")?;
        // `entry(...).or_default()` 如果 key 不存在则插入空 Vec，返回可变引用
        output.entry(post_id).or_default().push(tag);
    }
    Ok(output)
}

async fn blog_post_tags_for_ids(
    pool: &DbPool,
    ids: &[String],
) -> anyhow::Result<BTreeMap<String, Vec<String>>> {
    if ids.is_empty() {
        return Ok(BTreeMap::new());
    }
    let rows = query(
        "SELECT post_id, tag FROM blog_post_tags WHERE post_id = ANY($1) ORDER BY post_id, tag",
    )
    .bind(ids)
    .fetch_all(pool)
    .await?;
    let mut output = BTreeMap::<String, Vec<String>>::new();
    for row in rows {
        output
            .entry(row.try_get("post_id")?)
            .or_default()
            .push(row.try_get("tag")?);
    }
    Ok(output)
}

/// 查询单篇文章的标签列表（用于按路径查询单篇文章时）。
async fn tags_for_blog_post(pool: &DbPool, post_id: &str) -> anyhow::Result<Vec<String>> {
    let rows = query(
        r#"
        SELECT tag
        FROM blog_post_tags
        WHERE post_id = $1
        ORDER BY tag ASC
        "#,
    )
    .bind(post_id)
    .fetch_all(pool)
    .await?;

    rows.into_iter()
        .map(|row| row.try_get("tag").map_err(Into::into))
        .collect()
}

/// 将数据库行和标签映射转换为 `BlogPostRecord`。
///
/// `tags.get(&id).cloned().unwrap_or_default()` 的含义：
/// - `tags.get(&id)` 在 BTreeMap 中按 id 查找，返回 `Option<&Vec<String>>`
/// - `.cloned()` 将 `Option<&Vec<String>>` 转为 `Option<Vec<String>>`（复制数据）
/// - `.unwrap_or_default()` 如果找不到标签，返回空 Vec（没有标签的文章很正常）
fn blog_post_record_from_row(
    row: PgRow,
    tags: &BTreeMap<String, Vec<String>>,
) -> anyhow::Result<BlogPostRecord> {
    let id: String = row.try_get("id")?;
    Ok(BlogPostRecord {
        tags: tags.get(&id).cloned().unwrap_or_default(),
        id,
        relative_path: row.try_get("relative_path")?,
        extension: row.try_get("extension")?,
        title: row.try_get("title")?,
        description: row.try_get("description")?,
        content: row.try_get("content")?,
        draft: row.try_get("draft")?,
        featured: row.try_get("featured")?,
        pub_date: row.try_get("pub_date")?,
        updated_date: row.try_get("updated_date")?,
        author: row.try_get("author")?,
        category: row.try_get("category")?,
        series: row.try_get("series")?,
        hero_image: row.try_get("hero_image")?,
        updated_at: row.try_get("updated_at")?,
    })
}

/// 对字符串列表去重、去空白，保持首次出现的顺序。
///
/// 处理步骤：
/// 1. `trim()`：去除首尾空白（防止 " rust " 和 "rust" 被视为不同标签）
/// 2. `filter(|v| !v.is_empty())`：过滤空字符串
/// 3. `filter(|v| seen.insert(v.clone()))`：去重
///    - `BTreeSet::insert` 在元素不存在时插入并返回 `true`，已存在时返回 `false`
///    - 用 `filter` 只保留首次出现的元素，实现了"保序去重"
pub(crate) fn normalized_strings(values: &[String]) -> Vec<String> {
    let mut seen = std::collections::BTreeSet::new();
    values
        .iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .filter(|value| seen.insert(value.clone())) // 只保留首次出现的元素
        .collect()
}
