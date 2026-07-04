//! 博客后台 handler。
//!
//! 每个函数对应一个 HTTP 路由端点，负责接收请求、调用 `blog` 模块的业务逻辑、返回响应。
//!
//! # axum 提取器说明
//!
//! axum 框架使用"提取器"（Extractor）模式从请求中提取数据：
//! - `State(state): State<AppState>` — 提取全局应用状态（数据库连接、配置等）
//! - `Json(payload): Json<T>` — 将请求体 JSON 反序列化为类型 T
//! - `Query(query): Query<T>` — 将 URL 查询参数（如 `?path=xxx`）解析为类型 T
//!
//! 函数签名中的每个参数都是一个提取器，axum 自动按顺序执行它们。
//! 如果任何提取器失败（如 JSON 格式错误），axum 自动返回 400 Bad Request。

use axum::{
    Json, Router,
    extract::{Query, State},
    routing::{get, post},
};

use crate::{
    blog::{self, BlogPathQuery, PublishRequest},
    domain::{
        BlogBulkEditResponse, BlogCreateTaxonomyRequest, BlogDeleteCategoryRequest,
        BlogDeleteTagRequest, BlogHomeConfig, BlogPostSaveRequest, BlogRenameRequest, LogsResponse,
    },
    error::AppResult,
    service_manager,
    state::AppState,
};

use super::LogQuery;

pub(super) fn router() -> Router<AppState> {
    Router::new()
        .route("/api/blog/posts", get(posts).delete(delete_post))
        .route("/api/blog/posts/detail", get(post_detail))
        .route("/api/blog/posts/save", post(save_post))
        .route("/api/blog/home", get(home).post(save_home))
        .route("/api/blog/taxonomy", get(taxonomy))
        .route("/api/blog/build", post(build))
        .route("/api/blog/logs", get(logs))
        .route("/api/blog/publish", post(publish))
        .route("/api/blog/unpublish", post(unpublish))
        .route("/api/blog/tags/add", post(add_tag))
        .route("/api/blog/tags/rename", post(rename_tag))
        .route("/api/blog/tags/delete", post(delete_tag))
        .route("/api/blog/categories/add", post(add_category))
        .route("/api/blog/categories/rename", post(rename_category))
        .route("/api/blog/categories/delete", post(delete_category))
}

/// 列出所有博客文章（含草稿）。
pub(super) async fn posts(
    State(state): State<AppState>,
) -> AppResult<Json<Vec<crate::domain::BlogPost>>> {
    // `Ok(Json(...))` 将 Rust 值包装为 JSON 响应，axum 会自动设置 Content-Type: application/json
    Ok(Json(blog::list_posts(&state).await?))
}

/// 获取单篇博客文章的详细内容（含正文 Markdown）。
///
/// 通过 URL 查询参数 `?path=relative/path/to/post` 指定文章路径。
/// `Query(query): Query<BlogPathQuery>` 提取器负责解析 `?path=...` 参数。
pub(super) async fn post_detail(
    State(state): State<AppState>,
    Query(query): Query<BlogPathQuery>, // 从 URL 查询参数中提取 { path: String }
) -> AppResult<Json<crate::domain::BlogPostDetail>> {
    Ok(Json(blog::post_detail(&state, &query.path).await?))
}

/// 保存（新建或修改）一篇博客文章。
///
/// 如果文章不是草稿（`draft = false`），保存后自动触发后台构建，
/// 将最新内容发布到静态站点。
///
/// `payload: Json<BlogPostSaveRequest>` 中的 `.0` 是解包语法：
/// `Json<T>` 是一个元组结构体，`.0` 访问其内部的 `T` 值本身。
pub(super) async fn save_post(
    State(state): State<AppState>,
    payload: Json<BlogPostSaveRequest>,
) -> AppResult<Json<crate::domain::BlogPostWriteResponse>> {
    let is_published = !payload.0.draft; // 非草稿 = 已发布，需要触发构建
    let result = blog::save_post(&state, payload.0).await?;
    if is_published {
        // `trigger_background_build` 是非阻塞的：它在后台启动构建任务，
        // 不等构建完成就立即返回响应给客户端（构建通常需要几秒到几十秒）
        blog::trigger_background_build(state).await;
    }
    Ok(Json(result))
}

/// 删除一篇博客文章，并触发站点重新构建（从发布的静态站点中移除该文章）。
pub(super) async fn delete_post(
    State(state): State<AppState>,
    Query(query): Query<BlogPathQuery>,
) -> AppResult<Json<crate::domain::BlogPostDeleteResponse>> {
    let result = blog::delete_post(&state, &query.path).await?;
    blog::trigger_background_build(state).await; // 删除后必须重新构建，否则旧文章仍在静态站点上
    Ok(Json(result))
}

/// 获取所有分类和标签的候选列表（用于编辑器的下拉选择）。
pub(super) async fn taxonomy(
    State(state): State<AppState>,
) -> AppResult<Json<crate::domain::BlogTaxonomyResponse>> {
    Ok(Json(blog::taxonomy(&state).await?))
}

/// 获取博客首页配置（如站点名称、描述、精选文章设置等）。
pub(super) async fn home(State(state): State<AppState>) -> AppResult<Json<BlogHomeConfig>> {
    Ok(Json(blog::home_config(&state).await?))
}

/// 保存博客首页配置，并触发重新构建以应用更改。
pub(super) async fn save_home(
    State(state): State<AppState>,
    payload: Json<BlogHomeConfig>,
) -> AppResult<Json<BlogHomeConfig>> {
    let result = blog::save_home_config(&state, payload.0).await?;
    blog::trigger_background_build(state).await;
    Ok(Json(result))
}

/// 手动触发博客构建（将数据库中的文章生成为静态 HTML 文件）。
pub(super) async fn build(
    State(state): State<AppState>,
) -> AppResult<Json<crate::domain::BlogBuildResponse>> {
    Ok(Json(blog::build_blog(&state).await?))
}

/// 将指定文章标记为已发布（从草稿变为正式发布），并触发构建。
pub(super) async fn publish(
    State(state): State<AppState>,
    payload: Json<PublishRequest>,
) -> AppResult<Json<crate::domain::PublishResponse>> {
    let result = blog::publish_post(&state, payload.0).await?;
    blog::trigger_background_build(state).await;
    Ok(Json(result))
}

/// 将指定文章撤回为草稿状态（从已发布变为草稿），并触发构建以从站点移除。
pub(super) async fn unpublish(
    State(state): State<AppState>,
    payload: Json<PublishRequest>,
) -> AppResult<Json<crate::domain::PublishResponse>> {
    let result = blog::unpublish_post(&state, payload.0).await?;
    blog::trigger_background_build(state).await;
    Ok(Json(result))
}

/// 创建新标签（添加到标签候选库中）。
pub(super) async fn add_tag(
    State(state): State<AppState>,
    payload: Json<BlogCreateTaxonomyRequest>,
) -> AppResult<Json<BlogBulkEditResponse>> {
    Ok(Json(blog::create_tag(&state, payload.0).await?))
}

/// 重命名标签（同时更新所有引用该标签的文章）。
pub(super) async fn rename_tag(
    State(state): State<AppState>,
    payload: Json<BlogRenameRequest>,
) -> AppResult<Json<BlogBulkEditResponse>> {
    Ok(Json(blog::rename_tag(&state, payload.0).await?))
}

/// 删除标签（同时从所有引用该标签的文章中移除）。
pub(super) async fn delete_tag(
    State(state): State<AppState>,
    payload: Json<BlogDeleteTagRequest>,
) -> AppResult<Json<BlogBulkEditResponse>> {
    Ok(Json(blog::delete_tag(&state, payload.0).await?))
}

/// 创建新分类。
pub(super) async fn add_category(
    State(state): State<AppState>,
    payload: Json<BlogCreateTaxonomyRequest>,
) -> AppResult<Json<BlogBulkEditResponse>> {
    Ok(Json(blog::create_category(&state, payload.0).await?))
}

/// 重命名分类（同时更新所有属于该分类的文章）。
pub(super) async fn rename_category(
    State(state): State<AppState>,
    payload: Json<BlogRenameRequest>,
) -> AppResult<Json<BlogBulkEditResponse>> {
    Ok(Json(blog::rename_category(&state, payload.0).await?))
}

/// 删除分类（同时从所有属于该分类的文章中清除分类字段）。
pub(super) async fn delete_category(
    State(state): State<AppState>,
    payload: Json<BlogDeleteCategoryRequest>,
) -> AppResult<Json<BlogBulkEditResponse>> {
    Ok(Json(blog::delete_category(&state, payload.0).await?))
}

/// 读取最新一次博客构建的日志（取 build_log_dir 中最新的 .log 文件尾部 N 行）。
pub(super) async fn logs(
    State(state): State<AppState>,
    Query(query): Query<LogQuery>,
) -> AppResult<Json<LogsResponse>> {
    let lines = query.lines.unwrap_or(200).min(1000);
    let log_dir = &state.settings.blog.build_log_dir;

    // 找最近修改时间最新的 .log 文件
    let latest = std::fs::read_dir(log_dir).ok().and_then(|entries| {
        entries
            .flatten()
            .filter(|e| e.path().extension().and_then(|x| x.to_str()) == Some("log"))
            .max_by_key(|e| e.metadata().and_then(|m| m.modified()).ok())
    });

    let path = match latest {
        Some(entry) => entry.path(),
        None => {
            return Ok(Json(LogsResponse {
                path: log_dir.to_string_lossy().to_string(),
                lines: vec!["（暂无构建日志）".to_string()],
            }));
        }
    };

    Ok(Json(LogsResponse {
        path: path.to_string_lossy().to_string(),
        lines: service_manager::tail_lines(&path, lines)?,
    }))
}
