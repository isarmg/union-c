//! 博客数据库记录、导出文件和格式转换。

use super::{orphans::safe_content_path, *};

pub(super) async fn ensure_blog_seeded(state: &AppState) -> AppResult<()> {
    // 只补齐数据库中缺失的 taxonomy 和首页配置。
    // 文件导出由写操作（save_post / delete_post / rewrite_all_posts / build_blog）显式触发。
    sync_taxonomy_from_posts(state).await?;
    ensure_home_config_seeded(state).await?;
    Ok(())
}

pub(super) async fn sync_taxonomy_from_posts(state: &AppState) -> AppResult<()> {
    let posts = database::list_blog_posts(state.db().as_ref()).await?;

    let mut taxonomy_entries: Vec<(String, String)> = Vec::new();
    let mut category_tag_entries: Vec<(String, String)> = Vec::new();

    for post in &posts {
        if let Some(category) = post.category.as_deref() {
            taxonomy_entries.push((
                TaxonomyKind::Category.db_kind().to_string(),
                category.to_string(),
            ));
            for tag in &post.tags {
                category_tag_entries.push((category.to_string(), tag.clone()));
            }
        }
        for tag in &post.tags {
            taxonomy_entries.push((TaxonomyKind::Tag.db_kind().to_string(), tag.clone()));
        }
    }

    taxonomy_entries.sort_unstable();
    taxonomy_entries.dedup();
    category_tag_entries.sort_unstable();
    category_tag_entries.dedup();

    database::batch_insert_taxonomy(state.db().as_ref(), &taxonomy_entries).await?;
    database::batch_insert_category_tags(state.db().as_ref(), &category_tag_entries).await?;

    Ok(())
}

pub(super) async fn ensure_home_config_seeded(state: &AppState) -> AppResult<()> {
    if database::get_setting(state.db().as_ref(), BLOG_HOME_SETTING_KEY)
        .await?
        .is_some()
    {
        return Ok(());
    }

    save_home_config_to_db(state, &BlogHomeConfig::default()).await?;
    Ok(())
}

pub(super) async fn home_config_from_db(state: &AppState) -> AppResult<BlogHomeConfig> {
    let Some(value) = database::get_setting(state.db().as_ref(), BLOG_HOME_SETTING_KEY).await?
    else {
        return Ok(BlogHomeConfig::default());
    };
    let config: BlogHomeConfig = serde_json::from_str(&value).map_err(|err| {
        AppError::BadRequest(format!("invalid blog home config in database: {err}"))
    })?;
    let normalized = normalize_home_config(config.clone())?;
    if normalized != config {
        save_home_config_to_db(state, &normalized).await?;
    }
    Ok(normalized)
}

pub(super) async fn save_home_config_to_db(
    state: &AppState,
    config: &BlogHomeConfig,
) -> AppResult<()> {
    let content = serde_json::to_string(config)
        .map_err(|err| AppError::BadRequest(format!("invalid blog home config: {err}")))?;
    database::set_setting(state.db().as_ref(), BLOG_HOME_SETTING_KEY, &content).await?;
    Ok(())
}

pub(super) async fn export_blog_content(state: &AppState) -> AppResult<()> {
    let content_dir = &state.settings.paths.blog_export_dir;
    fs::create_dir_all(content_dir)?;

    for record in database::list_blog_posts(state.db().as_ref()).await? {
        export_post_record(content_dir, &record).await?;
    }
    export_taxonomy_registry(state).await?;
    let config = home_config_from_db(state).await?;
    save_home_config_file(content_dir, &config)?;

    Ok(())
}

pub(super) async fn export_taxonomy_registry(state: &AppState) -> AppResult<()> {
    let registry = load_taxonomy_registry_from_db(state).await?;
    save_taxonomy_registry(&state.settings.paths.blog_export_dir, registry)?;
    Ok(())
}

pub(super) async fn load_taxonomy_registry_from_db(
    state: &AppState,
) -> AppResult<TaxonomyRegistry> {
    Ok(TaxonomyRegistry {
        tags: database::list_blog_taxonomy(state.db().as_ref(), TaxonomyKind::Tag.db_kind())
            .await?,
        categories: database::list_blog_taxonomy(
            state.db().as_ref(),
            TaxonomyKind::Category.db_kind(),
        )
        .await?,
    })
}

pub(super) async fn replace_registered_taxonomy(
    state: &AppState,
    kind: TaxonomyKind,
    from: &str,
    to: &str,
) -> AppResult<bool> {
    let removed = database::delete_blog_taxonomy(state.db().as_ref(), kind.db_kind(), from).await?;
    let inserted = database::insert_blog_taxonomy(state.db().as_ref(), kind.db_kind(), to).await?;
    Ok(removed || inserted)
}

pub(super) fn save_taxonomy_registry(
    content_dir: &Path,
    mut registry: TaxonomyRegistry,
) -> AppResult<()> {
    fs::create_dir_all(content_dir)?;
    normalize_taxonomy_registry(&mut registry);
    let path = content_dir.join(TAXONOMY_REGISTRY_FILE);
    let content = serde_json::to_string_pretty(&registry)
        .map_err(|err| AppError::BadRequest(format!("invalid blog taxonomy registry: {err}")))?;
    atomic_write_file(&path, format!("{content}\n").as_bytes())?;
    Ok(())
}

pub(super) fn save_home_config_file(content_dir: &Path, config: &BlogHomeConfig) -> AppResult<()> {
    fs::create_dir_all(content_dir)?;
    let path = content_dir.join(BLOG_HOME_CONFIG_FILE);
    write_home_config_file(&path, config)
}

pub(super) fn write_home_config_file(path: &Path, config: &BlogHomeConfig) -> AppResult<()> {
    let content = serde_json::to_string_pretty(config)
        .map_err(|err| AppError::BadRequest(format!("invalid blog home config: {err}")))?;
    atomic_write_file(path, format!("{content}\n").as_bytes())?;
    Ok(())
}

pub(super) fn normalize_home_config(mut config: BlogHomeConfig) -> AppResult<BlogHomeConfig> {
    let defaults = BlogHomeConfig::default();
    config.site_url = clean_home_field(config.site_url, defaults.site_url);
    config.site_name = clean_home_field(config.site_name, defaults.site_name);
    config.site_title = clean_home_field(config.site_title, defaults.site_title);
    config.site_description = clean_home_field(config.site_description, defaults.site_description);
    config.hero_title = clean_home_field(config.hero_title, defaults.hero_title);
    config.hero_subtitle = clean_home_field(config.hero_subtitle, defaults.hero_subtitle);
    config.background_image =
        clean_home_asset_field(config.background_image, defaults.background_image);
    config.announcement = config.announcement.trim().to_string();
    config.avatar_image = clean_home_asset_field(config.avatar_image, defaults.avatar_image);
    config.footer_note = config.footer_note.trim().to_string();

    for (label, value, max_chars) in [
        ("site_url", config.site_url.as_str(), 240),
        ("site_name", config.site_name.as_str(), 80),
        ("site_title", config.site_title.as_str(), 120),
        ("site_description", config.site_description.as_str(), 240),
        ("hero_title", config.hero_title.as_str(), 80),
        ("hero_subtitle", config.hero_subtitle.as_str(), 180),
        ("background_image", config.background_image.as_str(), 512),
        ("announcement", config.announcement.as_str(), 240),
        ("avatar_image", config.avatar_image.as_str(), 512),
        ("footer_note", config.footer_note.as_str(), 240),
    ] {
        if value.chars().count() > max_chars {
            return Err(AppError::BadRequest(format!(
                "{label} cannot be longer than {max_chars} characters"
            )));
        }
    }

    Ok(config)
}

pub(super) fn clean_home_field(value: String, fallback: String) -> String {
    let value = value.trim();
    if value.is_empty() {
        fallback
    } else {
        value.to_string()
    }
}

pub(super) fn clean_home_asset_field(value: String, fallback: String) -> String {
    normalize_blog_asset_path_value(&clean_home_field(value, fallback))
}

pub(super) fn normalize_taxonomy_registry(registry: &mut TaxonomyRegistry) {
    registry.tags = normalize_list(registry.tags.clone());
    registry.categories = normalize_list(registry.categories.clone());
}

pub(super) fn normalize_taxonomy_name(name: &str, label: &str) -> AppResult<String> {
    let name = name.trim();
    if name.is_empty() {
        return Err(AppError::BadRequest(format!("{label} cannot be empty")));
    }
    if name.chars().count() > 64 {
        return Err(AppError::BadRequest(format!(
            "{label} cannot be longer than 64 characters"
        )));
    }
    if name.contains(['\n', '\r', ',', '，']) {
        return Err(AppError::BadRequest(format!(
            "{label} cannot contain line breaks or commas"
        )));
    }
    Ok(name.to_string())
}

impl TaxonomyKind {
    pub(super) fn label(self) -> &'static str {
        match self {
            TaxonomyKind::Tag => "tag name",
            TaxonomyKind::Category => "category name",
        }
    }

    pub(super) fn create_audit_action(self) -> &'static str {
        match self {
            TaxonomyKind::Tag => "blog.tag.create",
            TaxonomyKind::Category => "blog.category.create",
        }
    }

    pub(super) fn db_kind(self) -> &'static str {
        match self {
            TaxonomyKind::Tag => "tag",
            TaxonomyKind::Category => "category",
        }
    }
}

pub(super) fn post_input_from_request(request: &BlogPostSaveRequest) -> database::BlogPostInput {
    database::BlogPostInput {
        id: post_id_from_relative_path(&request.relative_path),
        relative_path: request.relative_path.trim().to_string(),
        extension: extension_from_relative_path(&request.relative_path),
        title: request.title.trim().to_string(),
        description: request.description.trim().to_string(),
        content: request.content.clone(),
        draft: request.draft,
        featured: request.featured,
        pub_date: Some(request.pub_date.trim().to_string()),
        updated_date: clean_optional(&request.updated_date),
        author: clean_optional(&request.author),
        category: clean_optional(&request.category),
        series: clean_optional(&request.series),
        hero_image: clean_blog_asset_path(&request.hero_image),
        tags: normalize_list(request.tags.clone()),
    }
}

pub(super) fn post_record_to_post(record: database::BlogPostRecord) -> BlogPost {
    BlogPost {
        id: record.id,
        title: record.title,
        description: record.description,
        relative_path: record.relative_path,
        extension: record.extension,
        draft: record.draft,
        featured: record.featured,
        pub_date: record.pub_date,
        updated_date: record.updated_date,
        author: record.author,
        category: record.category,
        series: record.series,
        hero_image: normalize_blog_asset_path(record.hero_image),
        tags: record.tags,
        updated_at: record.updated_at,
    }
}

pub(super) fn post_request_from_record(record: &database::BlogPostRecord) -> BlogPostSaveRequest {
    BlogPostSaveRequest {
        original_relative_path: Some(record.relative_path.clone()),
        relative_path: record.relative_path.clone(),
        title: record.title.clone(),
        description: record.description.clone(),
        pub_date: record
            .pub_date
            .clone()
            .unwrap_or_else(|| Utc::now().date_naive().to_string()),
        updated_date: record.updated_date.clone(),
        author: record.author.clone(),
        category: record.category.clone(),
        series: record.series.clone(),
        hero_image: normalize_blog_asset_path(record.hero_image.clone()),
        tags: record.tags.clone(),
        draft: record.draft,
        featured: record.featured,
        content: record.content.clone(),
    }
}

pub(super) fn front_from_record(record: &database::BlogPostRecord) -> FrontMatter {
    FrontMatter {
        title: Some(record.title.clone()),
        description: Some(record.description.clone()),
        pub_date: record.pub_date.clone(),
        updated_date: record.updated_date.clone(),
        author: record.author.clone(),
        category: record.category.clone(),
        series: record.series.clone(),
        hero_image: normalize_blog_asset_path(record.hero_image.clone()),
        tags: record.tags.clone(),
        draft: record.draft,
        featured: record.featured,
    }
}

pub(super) fn apply_front_to_request(request: &mut BlogPostSaveRequest, front: FrontMatter) {
    request.title = front.title.unwrap_or_else(|| "Untitled".to_string());
    request.description = front.description.unwrap_or_default();
    request.pub_date = front
        .pub_date
        .unwrap_or_else(|| Utc::now().date_naive().to_string());
    request.updated_date = front.updated_date;
    request.author = front.author;
    request.category = front.category;
    request.series = front.series;
    request.hero_image = normalize_blog_asset_path(front.hero_image);
    request.tags = front.tags;
    request.draft = front.draft;
    request.featured = front.featured;
}

pub(super) async fn export_post_record(
    content_dir: &Path,
    record: &database::BlogPostRecord,
) -> AppResult<()> {
    let request = post_request_from_record(record);
    export_post_request(content_dir, &request).await
}

pub(super) async fn export_post_request(
    content_dir: &Path,
    request: &BlogPostSaveRequest,
) -> AppResult<()> {
    fs::create_dir_all(content_dir)?;
    let path = safe_content_path(content_dir, &request.relative_path, false)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
        // 父目录创建后用 canonicalize 二次确认未通过符号链接逃出内容目录。
        let canonical_base = content_dir.canonicalize()?;
        let canonical_parent = parent.canonicalize()?;
        if !canonical_parent.starts_with(&canonical_base) {
            return Err(AppError::BadRequest(
                "blog path escapes content directory".to_string(),
            ));
        }
    }
    atomic_write_file(&path, render_post(request).as_bytes())?;
    Ok(())
}

pub(super) fn sibling_work_path(path: &Path, label: &str) -> AppResult<PathBuf> {
    let parent = path
        .parent()
        .ok_or_else(|| AppError::BadRequest("blog path has no parent".to_string()))?;
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("post");
    Ok(parent.join(format!(".{name}.{label}.{}", Uuid::new_v4())))
}

/// 把现有文件移到同一目录，后续 rename 可以保持原子性。
pub(super) fn stage_existing_file(path: &Path) -> AppResult<Option<PathBuf>> {
    if !path.exists() {
        return Ok(None);
    }
    let backup = sibling_work_path(path, "backup")?;
    fs::rename(path, &backup)?;
    Ok(Some(backup))
}

pub(super) fn atomic_write_file(path: &Path, content: &[u8]) -> AppResult<()> {
    let parent = path
        .parent()
        .ok_or_else(|| AppError::BadRequest("blog path has no parent".to_string()))?;
    fs::create_dir_all(parent)?;
    let temporary = sibling_work_path(path, "tmp")?;
    if let Err(err) = fs::write(&temporary, content).and_then(|_| fs::rename(&temporary, path)) {
        let _ = fs::remove_file(&temporary);
        return Err(err.into());
    }
    Ok(())
}

pub(super) fn restore_staged_file(path: &Path, backup: Option<&Path>) {
    let _ = fs::remove_file(path);
    if let Some(backup) = backup {
        let _ = fs::rename(backup, path);
    }
}

pub(super) fn discard_staged_file(backup: Option<&Path>) {
    if let Some(backup) = backup {
        let _ = fs::remove_file(backup);
    }
}

pub(super) fn post_id_from_relative_path(relative_path: &str) -> String {
    relative_path
        .trim()
        .trim_end_matches(".mdx")
        .trim_end_matches(".md")
        .to_string()
}

pub(super) fn extension_from_relative_path(relative_path: &str) -> String {
    Path::new(relative_path)
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("md")
        .to_string()
}

/// 文章正文最大允许 5 MiB，防止超大内容耗尽内存或撑爆数据库。
const MAX_CONTENT_BYTES: usize = 5 * 1024 * 1024;

/// 校验保存文章的请求。
///
/// 这里做基础校验，路径安全校验由 `safe_content_path` 完成。
pub(super) fn validate_post_request(request: &BlogPostSaveRequest) -> AppResult<()> {
    if request.title.trim().is_empty() {
        return Err(AppError::BadRequest("blog title is required".to_string()));
    }
    if request.description.trim().is_empty() {
        return Err(AppError::BadRequest(
            "blog description is required".to_string(),
        ));
    }
    if request.content.len() > MAX_CONTENT_BYTES {
        return Err(AppError::BadRequest(format!(
            "post content exceeds the maximum size of {} MiB",
            MAX_CONTENT_BYTES / 1024 / 1024
        )));
    }
    let pub_date = request.pub_date.trim();
    if pub_date.is_empty() {
        return Err(AppError::BadRequest("pubDate is required".to_string()));
    }
    if NaiveDate::parse_from_str(pub_date, "%Y-%m-%d").is_err() {
        return Err(AppError::BadRequest(format!(
            "pubDate must be a valid date in YYYY-MM-DD format, got: {pub_date}"
        )));
    }
    if !request.relative_path.ends_with(".md") && !request.relative_path.ends_with(".mdx") {
        return Err(AppError::BadRequest(
            "blog post path must end with .md or .mdx".to_string(),
        ));
    }
    Ok(())
}

pub(super) async fn validate_post_tags_for_category(
    state: &AppState,
    request: &BlogPostSaveRequest,
) -> AppResult<()> {
    let tags = normalize_list(request.tags.clone());
    if tags.is_empty() {
        return Ok(());
    }

    let Some(category) = clean_optional(&request.category) else {
        return Err(AppError::BadRequest(
            "select a category before choosing tags".to_string(),
        ));
    };

    let allowed_tags = database::list_blog_category_tags(state.db().as_ref())
        .await?
        .into_iter()
        .filter_map(|(item_category, tag)| (item_category == category).then_some(tag))
        .collect::<BTreeSet<_>>();

    let invalid_tags = tags
        .into_iter()
        .filter(|tag| !allowed_tags.contains(tag))
        .collect::<Vec<_>>();

    if invalid_tags.is_empty() {
        Ok(())
    } else {
        Err(AppError::BadRequest(format!(
            "tags do not belong to category {category}: {}",
            invalid_tags.join(", ")
        )))
    }
}

/// 根据前端提交的请求重新生成 Markdown/MDX 文件内容。
pub(super) fn render_post(request: &BlogPostSaveRequest) -> String {
    // 统一渲染 front matter，保证后台保存、发布状态切换和批量重写输出格式一致。
    let tags = normalize_list(request.tags.clone());
    let mut lines = Vec::new();
    lines.push(format!("title: {}", yaml_string(&request.title)));
    lines.push(format!(
        "description: {}",
        yaml_string(&request.description)
    ));
    lines.push(format!("pubDate: {}", request.pub_date.trim()));
    if let Some(value) = clean_optional(&request.updated_date) {
        lines.push(format!("updatedDate: {value}"));
    }
    if let Some(value) = clean_optional(&request.author) {
        lines.push(format!("author: {}", yaml_string(&value)));
    }
    if let Some(value) = clean_optional(&request.category) {
        lines.push(format!("category: {}", yaml_string(&value)));
    }
    if let Some(value) = clean_optional(&request.series) {
        lines.push(format!("series: {}", yaml_string(&value)));
    }
    lines.push(format!("tags: {}", yaml_array(&tags)));
    lines.push(format!("draft: {}", request.draft));
    lines.push(format!("featured: {}", request.featured));
    if let Some(value) = clean_blog_asset_path(&request.hero_image) {
        lines.push(format!("heroImage: {}", yaml_string(&value)));
    }

    format!(
        "---\n{}\n---\n\n{}",
        lines.join("\n"),
        request.content.trim_start()
    )
}

/// 把普通字符串转成安全的 YAML 字符串。
pub(super) fn yaml_string(value: &str) -> String {
    format!(
        "\"{}\"",
        value.trim().replace('\\', "\\\\").replace('"', "\\\"")
    )
}

/// 把字符串数组转成 YAML 行内数组。
pub(super) fn yaml_array(values: &[String]) -> String {
    format!(
        "[{}]",
        values
            .iter()
            .map(|value| yaml_string(value))
            .collect::<Vec<_>>()
            .join(", ")
    )
}

/// 清理列表：去空、去重、保留首次出现顺序。
pub(super) fn normalize_list(values: Vec<String>) -> Vec<String> {
    let mut seen = BTreeSet::new();
    values
        .into_iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .filter(|value| seen.insert(value.clone()))
        .collect()
}

/// 把统计 map 转成前端需要的数组，并按数量倒序。
pub(super) fn sorted_taxonomy(map: BTreeMap<String, usize>) -> Vec<BlogTaxonomyItem> {
    let mut items = map
        .into_iter()
        .map(|(name, count)| BlogTaxonomyItem { name, count })
        .collect::<Vec<_>>();
    items.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.name.cmp(&b.name)));
    items
}

/// 把可选字符串中的空字符串归一化成 None。
pub(super) fn clean_optional(value: &Option<String>) -> Option<String> {
    value
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

pub(super) fn clean_blog_asset_path(value: &Option<String>) -> Option<String> {
    clean_optional(value).map(|value| normalize_blog_asset_path_value(&value))
}

pub(super) fn normalize_blog_asset_path(value: Option<String>) -> Option<String> {
    value
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(normalize_blog_asset_path_value)
}

pub(super) fn normalize_blog_asset_path_value(value: &str) -> String {
    let value = value.trim();
    match value {
        "" => DEFAULT_BLOG_IMAGE_PATH.to_string(),
        value if value.starts_with(BLOG_ASSET_PREFIX) => value.to_string(),
        value if value.starts_with('/') => {
            format!("{BLOG_ASSET_PREFIX}{}", value.trim_start_matches('/'))
        }
        value if value.contains("://") || value.starts_with("//") => value.to_string(),
        value => format!("{BLOG_ASSET_PREFIX}{value}"),
    }
}

/// 判断路径是否是博客文章文件。
pub(super) fn is_post_path(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|value| value.to_str()),
        Some("md" | "mdx")
    )
}
