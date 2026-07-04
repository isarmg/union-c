//! 文章 CRUD 与发布状态。

use super::{orphans::validate_relative_post_path, storage::*, *};

/// 列出所有博客文章。
pub async fn list_posts(state: &AppState) -> AppResult<Vec<BlogPost>> {
    ensure_blog_seeded(state).await?;
    let posts = database::list_blog_posts(state.db().as_ref()).await?;
    Ok(posts.into_iter().map(post_record_to_post).collect())
}

/// 读取单篇文章详情。
///
/// 返回值包含文章元数据和正文；正文会去掉 frontmatter，方便前端编辑器直接编辑内容。
pub async fn post_detail(state: &AppState, path: &str) -> AppResult<BlogPostDetail> {
    ensure_blog_seeded(state).await?;
    validate_relative_post_path(&state.settings.paths.blog_export_dir, path, false)?;
    let record = database::blog_post_by_path(state.db().as_ref(), path)
        .await?
        .ok_or_else(|| AppError::BadRequest(format!("blog post does not exist: {path}")))?;

    Ok(BlogPostDetail {
        content: record.content.clone(),
        post: post_record_to_post(record),
    })
}

/// 新建或保存文章。
///
/// 保存前会校验标题、路径和正文，然后重新生成 frontmatter，保证文件格式稳定。
pub async fn save_post(
    state: &AppState,
    request: BlogPostSaveRequest,
) -> AppResult<BlogPostWriteResponse> {
    let _content_guard = state.blog.content_lock.lock().await;
    ensure_blog_seeded(state).await?;
    validate_post_request(&request)?;
    validate_post_tags_for_category(state, &request).await?;
    validate_relative_post_path(
        &state.settings.paths.blog_export_dir,
        &request.relative_path,
        false,
    )?;
    let input = post_input_from_request(&request);
    let original_path = request
        .original_relative_path
        .as_deref()
        .map(str::trim)
        .filter(|path| !path.is_empty());
    if let Some(original) = original_path {
        validate_relative_post_path(&state.settings.paths.blog_export_dir, original, false)?;
        if database::blog_post_by_path(state.db().as_ref(), original)
            .await?
            .is_none()
        {
            return Err(AppError::Conflict(format!(
                "original blog post no longer exists: {original}"
            )));
        }
        if original != input.relative_path
            && database::blog_post_by_path(state.db().as_ref(), &input.relative_path)
                .await?
                .is_some()
        {
            return Err(AppError::Conflict(format!(
                "target blog path already exists: {}",
                input.relative_path
            )));
        }
    } else if database::blog_post_by_path(state.db().as_ref(), &input.relative_path)
        .await?
        .is_some()
    {
        return Err(AppError::Conflict(format!(
            "target blog path already exists: {}",
            input.relative_path
        )));
    }

    let target_path = validate_relative_post_path(
        &state.settings.paths.blog_export_dir,
        &input.relative_path,
        false,
    )?;
    let original_file = original_path
        .filter(|path| *path != input.relative_path)
        .map(|path| validate_relative_post_path(&state.settings.paths.blog_export_dir, path, false))
        .transpose()?
        .filter(|path| path != &target_path);

    // 先把旧文件挪到同目录备份，再原子写入新文件。数据库失败时可以完整回滚。
    let target_backup = stage_existing_file(&target_path)?;
    let original_backup = if let Some(path) = original_file.as_deref() {
        match stage_existing_file(path) {
            Ok(backup) => backup,
            Err(err) => {
                restore_staged_file(&target_path, target_backup.as_deref());
                return Err(err);
            }
        }
    } else {
        None
    };
    if let Err(err) = atomic_write_file(&target_path, render_post(&request).as_bytes()) {
        restore_staged_file(&target_path, target_backup.as_deref());
        if let Some(path) = original_file.as_deref() {
            restore_staged_file(path, original_backup.as_deref());
        }
        return Err(err);
    }

    if let Err(err) =
        database::upsert_blog_post_from_path(state.db().as_ref(), &input, original_path).await
    {
        restore_staged_file(&target_path, target_backup.as_deref());
        if let Some(path) = original_file.as_deref() {
            restore_staged_file(path, original_backup.as_deref());
        }
        return Err(err.into());
    }
    discard_staged_file(target_backup.as_deref());
    discard_staged_file(original_backup.as_deref());
    export_taxonomy_registry(state).await?;

    database::insert_audit(
        state.db().as_ref(),
        "blog.post.save",
        &request.relative_path,
        Some(if request.draft { "draft" } else { "published" }),
    )
    .await?;

    let post = database::blog_post_by_path(state.db().as_ref(), &request.relative_path)
        .await?
        .map(post_record_to_post)
        .ok_or_else(|| AppError::BadRequest("saved blog post cannot be loaded".to_string()))?;

    Ok(BlogPostWriteResponse { saved: true, post })
}

/// 删除文章。
pub async fn delete_post(state: &AppState, path: &str) -> AppResult<BlogPostDeleteResponse> {
    let _content_guard = state.blog.content_lock.lock().await;
    ensure_blog_seeded(state).await?;
    let path_buf = validate_relative_post_path(&state.settings.paths.blog_export_dir, path, false)?;
    let staged = stage_existing_file(&path_buf)?;
    let deleted = match database::delete_blog_post(state.db().as_ref(), path).await {
        Ok(deleted) => deleted,
        Err(err) => {
            restore_staged_file(&path_buf, staged.as_deref());
            return Err(err.into());
        }
    };
    if !deleted {
        restore_staged_file(&path_buf, staged.as_deref());
        return Err(AppError::BadRequest(format!(
            "blog post does not exist: {path}"
        )));
    }
    discard_staged_file(staged.as_deref());
    export_taxonomy_registry(state).await?;
    database::insert_audit(state.db().as_ref(), "blog.post.delete", path, None).await?;
    Ok(BlogPostDeleteResponse {
        deleted,
        path: path.to_string(),
    })
}

/// 发布文章：把 draft 改成 false。
pub async fn publish_post(state: &AppState, request: PublishRequest) -> AppResult<PublishResponse> {
    set_post_draft(state, &request.path, false, "blog.publish").await
}

/// 取消发布文章：把 draft 改成 true。
pub async fn unpublish_post(
    state: &AppState,
    request: PublishRequest,
) -> AppResult<PublishResponse> {
    set_post_draft(state, &request.path, true, "blog.unpublish").await
}

/// 修改文章 draft 状态的共用函数。
async fn set_post_draft(
    state: &AppState,
    path: &str,
    draft: bool,
    audit_action: &str,
) -> AppResult<PublishResponse> {
    let _content_guard = state.blog.content_lock.lock().await;
    ensure_blog_seeded(state).await?;
    validate_relative_post_path(&state.settings.paths.blog_export_dir, path, false)?;
    let record = database::blog_post_by_path(state.db().as_ref(), path)
        .await?
        .ok_or_else(|| AppError::BadRequest(format!("blog post does not exist: {path}")))?;
    let changed = record.draft != draft;
    if changed {
        let mut request = post_request_from_record(&record);
        request.draft = draft;
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
    }

    database::insert_audit(
        state.db().as_ref(),
        audit_action,
        path,
        Some(if draft { "draft=true" } else { "draft=false" }),
    )
    .await?;

    Ok(PublishResponse {
        path: path.to_string(),
        changed,
    })
}
