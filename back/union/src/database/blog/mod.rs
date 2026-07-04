//! 博客文章、分类、标签的数据库操作。
//!
//! # 数据库表结构
//!
//! - `blog_posts`：博客文章主表（一对一，每篇文章一行）
//! - `blog_post_tags`：文章-标签关联表（多对多，一篇文章可有多个标签）
//! - `blog_taxonomy`：标签/分类的候选值列表（前端下拉菜单的数据来源）
//! - `blog_category_tags`：分类下的可选标签关联（某分类常用的标签集合）
//!
//! # 标签 vs 分类
//!
//! - `tags`：自由标签，一篇文章可有多个（存在 `blog_post_tags` 关联表）
//! - `category`：单一分类，一篇文章只属于一个分类（直接存在 `blog_posts.category` 列）

use std::collections::BTreeMap;

use sqlx_core::{query::query, row::Row};
use sqlx_postgres::{PgConnection, PgRow};

use super::DbPool;

// ─── 输入/输出类型 ────────────────────────────────────────────────────────────

/// 写入数据库时使用的博客文章数据结构（不含 `updated_at` 等由数据库自动填充的字段）。
#[derive(Debug, Clone)]
pub struct BlogPostInput {
    pub id: String,            // 文章唯一 ID（通常基于文件路径生成）
    pub relative_path: String, // 相对于博客根目录的文件路径（如 "posts/hello-world.md"）
    pub extension: String,     // 文件扩展名（"md" 或 "mdx"）
    pub title: String,
    pub description: String,
    pub content: String,              // Markdown/MDX 正文内容
    pub draft: bool,                  // true = 草稿（不显示在公开站点），false = 已发布
    pub featured: bool,               // 是否在首页精选展示
    pub pub_date: Option<String>,     // 发布日期（ISO 8601 格式字符串，如 "2024-01-01"）
    pub updated_date: Option<String>, // 最后更新日期
    pub author: Option<String>,
    pub category: Option<String>,   // 单一分类名称
    pub series: Option<String>,     // 系列名称（同一系列的文章可以分组显示）
    pub hero_image: Option<String>, // 头图 URL
    pub tags: Vec<String>,          // 标签列表（存储在关联表）
}

/// 从数据库读取的博客文章记录（比 Input 多了 `updated_at` 字段）。
#[derive(Debug, Clone)]
pub struct BlogPostRecord {
    pub id: String,
    pub relative_path: String,
    pub extension: String,
    pub title: String,
    pub description: String,
    pub content: String,
    pub draft: bool,
    pub featured: bool,
    pub pub_date: Option<String>,
    pub updated_date: Option<String>,
    pub author: Option<String>,
    pub category: Option<String>,
    pub series: Option<String>,
    pub hero_image: Option<String>,
    pub tags: Vec<String>,
    pub updated_at: Option<String>, // 数据库自动维护的最后写入时间（UTC ISO 8601 格式）
}

mod posts;
mod taxonomy;

pub use posts::{
    blog_post_by_path, delete_blog_post, list_blog_posts, list_blog_posts_page,
    list_blog_posts_with_content, upsert_blog_post, upsert_blog_post_from_path, upsert_blog_posts,
};
pub use taxonomy::{
    batch_insert_category_tags, batch_insert_taxonomy, delete_blog_category_tag,
    delete_blog_category_tag_everywhere, delete_blog_category_tags_for_category,
    delete_blog_taxonomy, insert_blog_category_tag, insert_blog_taxonomy, list_blog_category_tags,
    list_blog_taxonomy, rename_blog_category_tag, rename_blog_category_tag_everywhere,
    rename_blog_category_tags_category,
};
