//! 文章 CRUD 与发布状态。

use super::{orphans::validate_relative_post_path, storage::*, *};

/// 列出所有博客文章。
pub async fn list_posts(state: &AppState) -> AppResult<Vec<BlogPost>> {
    let posts = database::list_blog_posts(state.db().as_ref()).await?;
    Ok(posts.into_iter().map(post_record_to_post).collect())
}

pub async fn list_posts_page(
    state: &AppState,
    limit: i64,
    offset: i64,
) -> AppResult<Vec<BlogPost>> {
    let posts = database::list_blog_posts_page(state.db().as_ref(), limit, offset).await?;
    Ok(posts.into_iter().map(post_record_to_post).collect())
}

/// 读取单篇文章详情。
///
/// 返回值包含文章元数据和正文；正文会去掉 frontmatter，方便前端编辑器直接编辑内容。
pub async fn post_detail(state: &AppState, path: &str) -> AppResult<BlogPostDetail> {
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

    database::upsert_blog_post_from_path(state.db().as_ref(), &input, original_path).await?;
    export_blog_content(state).await?;

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
    validate_relative_post_path(&state.settings.paths.blog_export_dir, path, false)?;
    let deleted = database::delete_blog_post(state.db().as_ref(), path).await?;
    if !deleted {
        return Err(AppError::BadRequest(format!(
            "blog post does not exist: {path}"
        )));
    }
    export_blog_content(state).await?;
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
        database::upsert_blog_post(state.db().as_ref(), &post_input_from_request(&request)).await?;
        export_blog_content(state).await?;
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
