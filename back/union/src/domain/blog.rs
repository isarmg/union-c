use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize)]
pub struct BlogPost {
    pub id: String,
    pub title: String,
    pub description: String,
    pub relative_path: String,
    pub extension: String,
    pub draft: bool,
    pub featured: bool,
    pub pub_date: Option<String>,
    pub updated_date: Option<String>,
    pub author: Option<String>,
    pub category: Option<String>,
    pub series: Option<String>,
    pub hero_image: Option<String>,
    pub tags: Vec<String>,
    pub updated_at: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct BlogPostDetail {
    pub post: BlogPost,
    pub content: String,
}

#[derive(Debug, Deserialize)]
pub struct BlogPostSaveRequest {
    /// 编辑现有文章时的原路径，用于识别重命名。
    #[serde(default)]
    pub original_relative_path: Option<String>,
    pub relative_path: String,
    pub title: String,
    pub description: String,
    pub pub_date: String,
    pub updated_date: Option<String>,
    pub author: Option<String>,
    pub category: Option<String>,
    pub series: Option<String>,
    pub hero_image: Option<String>,
    pub tags: Vec<String>,
    pub draft: bool,
    pub featured: bool,
    pub content: String,
}

#[derive(Debug, Serialize)]
pub struct BlogPostWriteResponse {
    pub saved: bool,
    pub post: BlogPost,
}

#[derive(Debug, Serialize)]
pub struct BlogPostDeleteResponse {
    pub deleted: bool,
    pub path: String,
}

#[derive(Debug, Serialize)]
pub struct BlogTaxonomyItem {
    pub name: String,
    pub count: usize,
}

#[derive(Debug, Serialize)]
pub struct BlogCategoryTags {
    pub category: String,
    pub tags: Vec<BlogTaxonomyItem>,
}

#[derive(Debug, Serialize)]
pub struct BlogTaxonomyResponse {
    pub tags: Vec<BlogTaxonomyItem>,
    pub categories: Vec<BlogTaxonomyItem>,
    pub category_tags: Vec<BlogCategoryTags>,
}

#[derive(Debug, Deserialize)]
pub struct BlogRenameRequest {
    pub from: String,
    pub to: String,
    pub category: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct BlogDeleteTagRequest {
    pub tag: String,
    pub category: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct BlogCreateTaxonomyRequest {
    pub name: String,
    pub category: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct BlogDeleteCategoryRequest {
    pub category: String,
}

#[derive(Debug, Serialize)]
pub struct BlogBulkEditResponse {
    pub changed: usize,
}

#[derive(Debug, Serialize)]
pub struct BlogBuildResponse {
    pub job_id: String,
    pub success: bool,
    pub exit_code: Option<i32>,
    pub duration_ms: u64,
    pub log_path: String,
    pub log_tail: Vec<String>,
    pub adopted_as_drafts: usize,
}

pub const DEFAULT_BLOG_IMAGE_PATH: &str = "/blog-assets/images/home-lab-hero.png";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BlogHomeConfig {
    pub site_url: String,
    pub site_name: String,
    pub site_title: String,
    pub site_description: String,
    pub hero_title: String,
    pub hero_subtitle: String,
    pub background_image: String,
    pub announcement: String,
    pub avatar_image: String,
    pub footer_note: String,
}

impl Default for BlogHomeConfig {
    fn default() -> Self {
        Self {
            site_url: "http://home.lan:8090".to_string(),
            site_name: "Poetic Notes".to_string(),
            site_title: "Poetic Notes".to_string(),
            site_description: "有诗意地记录生活、技术、局域网自托管和长期维护的个人博客。"
                .to_string(),
            hero_title: "有诗意地记录".to_string(),
            hero_subtitle: "把生活、服务和长期维护写清楚".to_string(),
            background_image: DEFAULT_BLOG_IMAGE_PATH.to_string(),
            announcement: "文章草稿可以在控制台维护，发布后会自动进入博客首页。".to_string(),
            avatar_image: DEFAULT_BLOG_IMAGE_PATH.to_string(),
            footer_note: "Built with Astro, MDX and a local-first publishing workflow.".to_string(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct PublishResponse {
    pub path: String,
    pub changed: bool,
}
