//! 分类、标签和首页展示配置。

use super::{orphans::validate_relative_post_path, storage::*, *};

/// 统计标签和分类。
///
/// BTreeMap 会按键排序，最后前端看到的标签/分类顺序比较稳定。
pub async fn taxonomy(state: &AppState) -> AppResult<BlogTaxonomyResponse> {
    ensure_blog_seeded(state).await?;
    let posts = list_posts(state).await?;
    let mut tags = BTreeMap::<String, usize>::new();
    let mut categories = BTreeMap::<String, usize>::new();
    let mut category_tags = BTreeMap::<String, BTreeMap<String, usize>>::new();
    let registry = load_taxonomy_registry_from_db(state).await?;

    for tag in registry.tags {
        tags.entry(tag).or_default();
    }
    for category in registry.categories {
        categories.entry(category.clone()).or_default();
        category_tags.entry(category).or_default();
    }

    for (category, tag) in database::list_blog_category_tags(state.db().as_ref()).await? {
        categories.entry(category.clone()).or_default();
        tags.entry(tag.clone()).or_default();
        category_tags
            .entry(category)
            .or_default()
            .entry(tag)
            .or_default();
    }

    for post in posts {
        if let Some(category) = post.category {
            *categories.entry(category.clone()).or_default() += 1;
            let category_bucket = category_tags.entry(category).or_default();
            for tag in post.tags {
                *tags.entry(tag.clone()).or_default() += 1;
                *category_bucket.entry(tag).or_default() += 1;
            }
        } else {
            for tag in post.tags {
                *tags.entry(tag).or_default() += 1;
            }
        }
    }

    Ok(BlogTaxonomyResponse {
        tags: sorted_taxonomy(tags),
        categories: sorted_taxonomy(categories),
        category_tags: category_tags
            .into_iter()
            .map(|(category, tags)| BlogCategoryTags {
                category,
                tags: sorted_taxonomy(tags),
            })
            .collect(),
    })
}

/// 读取博客前台首页配置。
pub async fn home_config(state: &AppState) -> AppResult<BlogHomeConfig> {
    ensure_home_config_seeded(state).await?;
    home_config_from_db(state).await
}

/// 保存博客前台首页配置。
pub async fn save_home_config(
    state: &AppState,
    request: BlogHomeConfig,
) -> AppResult<BlogHomeConfig> {
    let _content_guard = state.blog.content_lock.lock().await;
    let config = normalize_home_config(request)?;
    let content_dir = &state.settings.paths.blog_export_dir;
    fs::create_dir_all(content_dir)?;
    let config_path = content_dir.join(BLOG_HOME_CONFIG_FILE);
    let backup = stage_existing_file(&config_path)?;
    if let Err(err) = write_home_config_file(&config_path, &config) {
        restore_staged_file(&config_path, backup.as_deref());
        return Err(err);
    }
    if let Err(err) = save_home_config_to_db(state, &config).await {
        restore_staged_file(&config_path, backup.as_deref());
        return Err(err);
    }
    discard_staged_file(backup.as_deref());
    database::insert_audit(
        state.db().as_ref(),
        "blog.home.save",
        &config.site_name,
        Some(&config.site_title),
    )
    .await?;
    Ok(config)
}

/// 新增一个可选标签，即使暂时没有文章使用也会出现在后台候选列表。
pub async fn create_tag(
    state: &AppState,
    request: BlogCreateTaxonomyRequest,
) -> AppResult<BlogBulkEditResponse> {
    ensure_blog_seeded(state).await?;
    let name = normalize_taxonomy_name(&request.name, TaxonomyKind::Tag.label())?;
    let tag_changed =
        database::insert_blog_taxonomy(state.db().as_ref(), TaxonomyKind::Tag.db_kind(), &name)
            .await?;
    let mut changed = usize::from(tag_changed);
    let mut detail = if tag_changed {
        "created".to_string()
    } else {
        "already_exists".to_string()
    };

    if let Some(category) = clean_optional(&request.category) {
        database::insert_blog_taxonomy(
            state.db().as_ref(),
            TaxonomyKind::Category.db_kind(),
            &category,
        )
        .await?;
        if database::insert_blog_category_tag(state.db().as_ref(), &category, &name).await? {
            changed += 1;
        }
        detail = format!("{detail} category={category}");
    }

    export_taxonomy_registry(state).await?;
    database::insert_audit(state.db().as_ref(), "blog.tag.create", &name, Some(&detail)).await?;

    Ok(BlogBulkEditResponse { changed })
}

/// 新增一个可选分类，即使暂时没有文章使用也会出现在后台候选列表。
pub async fn create_category(
    state: &AppState,
    request: BlogCreateTaxonomyRequest,
) -> AppResult<BlogBulkEditResponse> {
    create_taxonomy_item(state, TaxonomyKind::Category, request.name).await
}

/// 批量重命名标签。
///
/// 只改 frontmatter 中的 tags 数组，不直接改正文内容。
pub async fn rename_tag(
    state: &AppState,
    request: BlogRenameRequest,
) -> AppResult<BlogBulkEditResponse> {
    ensure_blog_seeded(state).await?;
    let from = normalize_taxonomy_name(&request.from, "tag name")?;
    let to = normalize_taxonomy_name(&request.to, "tag name")?;
    if from == to {
        return Ok(BlogBulkEditResponse { changed: 0 });
    }
    let category = clean_optional(&request.category);

    let changed = rewrite_all_posts(state, |front| {
        if let Some(category) = category.as_deref()
            && front.category.as_deref() != Some(category)
        {
            return false;
        }
        let mut did_change = false;
        let mut tags = BTreeSet::new();
        for tag in &front.tags {
            if tag == &from {
                tags.insert(to.clone());
                did_change = true;
            } else {
                tags.insert(tag.clone());
            }
        }
        if did_change {
            front.tags = tags.into_iter().collect();
        }
        did_change
    })
    .await?;
    let changed = if let Some(category) = category.as_deref() {
        let tag_changed =
            database::insert_blog_taxonomy(state.db().as_ref(), TaxonomyKind::Tag.db_kind(), &to)
                .await?;
        let relation_changed =
            database::rename_blog_category_tag(state.db().as_ref(), category, &from, &to).await?
                > 0;
        changed + usize::from(tag_changed) + usize::from(relation_changed)
    } else {
        let registry_changed =
            replace_registered_taxonomy(state, TaxonomyKind::Tag, &from, &to).await?;
        let relation_changed =
            database::rename_blog_category_tag_everywhere(state.db().as_ref(), &from, &to).await?
                > 0;
        changed + usize::from(registry_changed) + usize::from(relation_changed)
    };
    export_taxonomy_registry(state).await?;

    database::insert_audit(
        state.db().as_ref(),
        "blog.tag.rename",
        &from,
        Some(&format!(
            "to={to} category={} changed={changed}",
            category.as_deref().unwrap_or("*")
        )),
    )
    .await?;
    Ok(BlogBulkEditResponse { changed })
}

/// 批量删除标签。
pub async fn delete_tag(
    state: &AppState,
    request: BlogDeleteTagRequest,
) -> AppResult<BlogBulkEditResponse> {
    ensure_blog_seeded(state).await?;
    let tag = normalize_taxonomy_name(&request.tag, "tag name")?;
    let category = clean_optional(&request.category);

    let changed = rewrite_all_posts(state, |front| {
        if let Some(category) = category.as_deref()
            && front.category.as_deref() != Some(category)
        {
            return false;
        }
        let before = front.tags.len();
        front.tags.retain(|item| item != &tag);
        front.tags.len() != before
    })
    .await?;
    let changed = if let Some(category) = category.as_deref() {
        let relation_changed =
            database::delete_blog_category_tag(state.db().as_ref(), category, &tag).await?;
        changed + usize::from(relation_changed)
    } else {
        let registry_changed =
            database::delete_blog_taxonomy(state.db().as_ref(), TaxonomyKind::Tag.db_kind(), &tag)
                .await?;
        let relation_changed =
            database::delete_blog_category_tag_everywhere(state.db().as_ref(), &tag).await? > 0;
        changed + usize::from(registry_changed) + usize::from(relation_changed)
    };
    export_taxonomy_registry(state).await?;

    database::insert_audit(
        state.db().as_ref(),
        "blog.tag.delete",
        &tag,
        Some(&format!(
            "category={} changed={changed}",
            category.as_deref().unwrap_or("*")
        )),
    )
    .await?;
    Ok(BlogBulkEditResponse { changed })
}

/// 批量重命名分类。
pub async fn rename_category(
    state: &AppState,
    request: BlogRenameRequest,
) -> AppResult<BlogBulkEditResponse> {
    ensure_blog_seeded(state).await?;
    let from = normalize_taxonomy_name(&request.from, "category name")?;
    let to = normalize_taxonomy_name(&request.to, "category name")?;
    if from == to {
        return Ok(BlogBulkEditResponse { changed: 0 });
    }

    let changed = rewrite_all_posts(state, |front| {
        if front.category.as_deref() == Some(from.as_str()) {
            front.category = Some(to.clone());
            true
        } else {
            false
        }
    })
    .await?;
    let registry_changed =
        replace_registered_taxonomy(state, TaxonomyKind::Category, &from, &to).await?;
    let relation_changed =
        database::rename_blog_category_tags_category(state.db().as_ref(), &from, &to).await? > 0;
    let changed = changed + usize::from(registry_changed) + usize::from(relation_changed);
    export_taxonomy_registry(state).await?;

    database::insert_audit(
        state.db().as_ref(),
        "blog.category.rename",
        &from,
        Some(&format!("to={to} changed={changed}")),
    )
    .await?;
    Ok(BlogBulkEditResponse { changed })
}

/// 删除分类：清空使用该分类的文章 frontmatter，并移除后台候选项。
pub async fn delete_category(
    state: &AppState,
    request: BlogDeleteCategoryRequest,
) -> AppResult<BlogBulkEditResponse> {
    ensure_blog_seeded(state).await?;
    let category = normalize_taxonomy_name(&request.category, "category name")?;

    let changed = rewrite_all_posts(state, |front| {
        if front.category.as_deref() == Some(category.as_str()) {
            front.category = None;
            true
        } else {
            false
        }
    })
    .await?;
    let registry_changed = database::delete_blog_taxonomy(
        state.db().as_ref(),
        TaxonomyKind::Category.db_kind(),
        &category,
    )
    .await?;
    let relation_changed =
        database::delete_blog_category_tags_for_category(state.db().as_ref(), &category).await? > 0;
    let changed = changed + usize::from(registry_changed) + usize::from(relation_changed);
    export_taxonomy_registry(state).await?;

    database::insert_audit(
        state.db().as_ref(),
        "blog.category.delete",
        &category,
        Some(&format!("changed={changed}")),
    )
    .await?;
    Ok(BlogBulkEditResponse { changed })
}

/// 遍历所有文章并按传入闭包改写内容。
///
/// 标签改名、删除标签、分类改名都可以复用这个函数，避免三处重复读写文件。
async fn rewrite_all_posts<F>(state: &AppState, mut update: F) -> AppResult<usize>
where
    F: FnMut(&mut FrontMatter) -> bool,
{
    let _content_guard = state.blog.content_lock.lock().await;
    ensure_blog_seeded(state).await?;
    let records = database::list_blog_posts(state.db().as_ref()).await?;
    let mut changed = 0;
    for record in records {
        let mut front = front_from_record(&record);
        if update(&mut front) {
            let mut request = post_request_from_record(&record);
            apply_front_to_request(&mut request, front);
            let file_path = validate_relative_post_path(
                &state.settings.paths.blog_export_dir,
                &request.relative_path,
                false,
            )?;
            let backup = stage_existing_file(&file_path)?;
            if let Err(err) = atomic_write_file(&file_path, render_post(&request).as_bytes()) {
                restore_staged_file(&file_path, backup.as_deref());
                return Err(err);
            }
            if let Err(err) =
                database::upsert_blog_post(state.db().as_ref(), &post_input_from_request(&request))
                    .await
            {
                restore_staged_file(&file_path, backup.as_deref());
                return Err(err.into());
            }
            discard_staged_file(backup.as_deref());
            changed += 1;
        }
    }
    Ok(changed)
}

async fn create_taxonomy_item(
    state: &AppState,
    kind: TaxonomyKind,
    name: String,
) -> AppResult<BlogBulkEditResponse> {
    ensure_blog_seeded(state).await?;
    let name = normalize_taxonomy_name(&name, kind.label())?;
    let changed =
        database::insert_blog_taxonomy(state.db().as_ref(), kind.db_kind(), &name).await?;
    export_taxonomy_registry(state).await?;

    database::insert_audit(
        state.db().as_ref(),
        kind.create_audit_action(),
        &name,
        Some(if changed { "created" } else { "already_exists" }),
    )
    .await?;

    Ok(BlogBulkEditResponse {
        changed: usize::from(changed),
    })
}
