//! 博客内容管理。
//!
//! 博客文章、分类、标签和首页配置以 PostgreSQL 为管理源。
//! Astro 前台读取 `blog/data/content`，这个目录由数据库内容生成。

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Component, Path, PathBuf},
    time::Instant,
};

use tokio::{io::AsyncReadExt, sync::Semaphore};

/// 防止同时运行多次博客构建的全局信号量。
static BUILD_SEMAPHORE: Semaphore = Semaphore::const_new(1);

use chrono::{NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use tokio::process::Command;
use uuid::Uuid;

use crate::{
    database,
    domain::{
        BlogBuildResponse, BlogBulkEditResponse, BlogCategoryTags, BlogCreateTaxonomyRequest,
        BlogDeleteCategoryRequest, BlogDeleteTagRequest, BlogHomeConfig, BlogPost,
        BlogPostDeleteResponse, BlogPostDetail, BlogPostSaveRequest, BlogPostWriteResponse,
        BlogRenameRequest, BlogTaxonomyItem, BlogTaxonomyResponse, DEFAULT_BLOG_IMAGE_PATH,
        PublishResponse,
    },
    error::{AppError, AppResult},
    service_manager::tail_lines,
    state::AppState,
};

const TAXONOMY_REGISTRY_FILE: &str = ".taxonomy.json";
const BLOG_HOME_CONFIG_FILE: &str = ".site.json";
const BLOG_HOME_SETTING_KEY: &str = "blog.home_config";
const BLOG_ASSET_PREFIX: &str = "/blog-assets/";
const MAX_BUILD_OUTPUT_BYTES: usize = 8 * 1024 * 1024;

#[derive(Debug, Deserialize)]
/// 发布/取消发布接口的请求体。
pub struct PublishRequest {
    /// 文章相对路径，例如 `back-start.md`。
    pub path: String,
}

#[derive(Debug, Deserialize)]
/// 通过查询字符串传入文章路径的接口参数。
pub struct BlogPathQuery {
    /// 文章相对路径。后续必须经过 `safe_content_path` 校验。
    pub path: String,
}

#[derive(Debug, Default, Deserialize)]
pub struct BlogListQuery {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Debug, Clone, Default)]
/// 从数据库记录中派生出的文章元数据。
///
/// 批量重命名标签、分类时只需要改这些元数据字段，不改正文。
struct FrontMatter {
    title: Option<String>,
    description: Option<String>,
    pub_date: Option<String>,
    updated_date: Option<String>,
    author: Option<String>,
    category: Option<String>,
    series: Option<String>,
    hero_image: Option<String>,
    tags: Vec<String>,
    draft: bool,
    featured: bool,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
struct TaxonomyRegistry {
    tags: Vec<String>,
    categories: Vec<String>,
}

#[derive(Debug, Clone, Copy)]
enum TaxonomyKind {
    Tag,
    Category,
}

mod build;
mod orphans;
mod posts;
mod storage;
mod taxonomy;

pub use build::{build_blog, trigger_background_build};
pub use orphans::adopt_orphan_posts;
pub use posts::{
    delete_post, list_posts, list_posts_page, post_detail, publish_post, save_post, unpublish_post,
};
pub use storage::ensure_blog_seeded;
pub use taxonomy::{
    create_category, create_tag, delete_category, delete_tag, home_config, rename_category,
    rename_tag, save_home_config, taxonomy,
};

/// 显式导入内容目录中未纳管的文章；正常启动和构建不会反向写入数据库。
pub async fn import_orphan_posts(state: &AppState) -> AppResult<usize> {
    let _content_guard = state.blog.content_lock.lock().await;
    let imported = adopt_orphan_posts(state).await?;
    storage::export_blog_content(state).await?;
    Ok(imported)
}
