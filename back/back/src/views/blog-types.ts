// 博客模块共用类型、常量与纯函数。

import type { QueryClient } from "@tanstack/react-query";
import { api } from "../api";
import { queryKeys } from "../query-keys";
import type { BlogHomeConfig, BlogPost, BlogPostSaveRequest } from "../types";

// ─── 常量 ─────────────────────────────────────────────────────────────────────

export const DEFAULT_BLOG_IMAGE_PATH = "/blog-assets/images/home-lab-hero.png";
export const BLOG_ASSET_URL_PREFIX = "/blog-assets/";
export const DEFAULT_BLOG_IMAGE_DISPLAY_PATH = "images/home-lab-hero.png";

// ─── 内部枚举类型 ─────────────────────────────────────────────────────────────

export type BlogStatusFilter = "all" | "draft" | "published";
export type BlogAdminSection = "home" | "published" | "draft" | "new" | "taxonomy";
export type BlogPostSection = "list" | "editor";

export type BlogDraft = {
  original_relative_path: string | null;
  relative_path: string;
  title: string;
  description: string;
  pub_date: string;
  updated_date: string;
  author: string;
  category: string;
  series: string;
  hero_image: string;
  tagsText: string;
  draft: boolean;
  featured: boolean;
  content: string;
  pathTouched: boolean;
};

// ─── 工厂函数 ─────────────────────────────────────────────────────────────────

export function emptyBlogDraft(): BlogDraft {
  return {
    original_relative_path: null,
    relative_path: `post-${new Date().toISOString().slice(0, 10)}.md`,
    title: "",
    description: "",
    pub_date: new Date().toISOString().slice(0, 10),
    updated_date: "",
    author: "Local Control",
    category: "",
    series: "",
    hero_image: DEFAULT_BLOG_IMAGE_DISPLAY_PATH,
    tagsText: "",
    draft: true,
    featured: false,
    content: "## 摘要\n\n在这里编写正文。\n",
    pathTouched: false
  };
}

export function emptyBlogHomeConfig(): BlogHomeConfig {
  return {
    site_url: "http://home.lan:8090",
    site_name: "Poetic Notes",
    site_title: "Poetic Notes",
    site_description: "有诗意地记录生活、技术、局域网自托管和长期维护的个人博客。",
    hero_title: "有诗意地记录",
    hero_subtitle: "把生活、服务和长期维护写清楚",
    background_image: DEFAULT_BLOG_IMAGE_DISPLAY_PATH,
    announcement: "文章草稿可以在控制台维护，发布后会自动进入博客首页。",
    avatar_image: DEFAULT_BLOG_IMAGE_DISPLAY_PATH,
    footer_note: "Built with Astro, MDX and a local-first publishing workflow."
  };
}

// ─── 转换函数 ─────────────────────────────────────────────────────────────────

export function toBlogHomeDraft(config: BlogHomeConfig): BlogHomeConfig {
  return {
    ...config,
    background_image: toBlogAssetDisplayPath(config.background_image),
    avatar_image: toBlogAssetDisplayPath(config.avatar_image)
  };
}

export function toBlogHomeSaveRequest(draft: BlogHomeConfig): BlogHomeConfig {
  return {
    ...draft,
    background_image: normalizeBlogAssetPath(draft.background_image) || DEFAULT_BLOG_IMAGE_PATH,
    avatar_image: normalizeBlogAssetPath(draft.avatar_image) || DEFAULT_BLOG_IMAGE_PATH
  };
}

export function toBlogDraft(detail: Awaited<ReturnType<typeof api.blogPostDetail>>): BlogDraft {
  return {
    original_relative_path: detail.post.relative_path,
    relative_path: detail.post.relative_path,
    title: detail.post.title,
    description: detail.post.description,
    pub_date: detail.post.pub_date?.slice(0, 10) ?? new Date().toISOString().slice(0, 10),
    updated_date: detail.post.updated_date?.slice(0, 10) ?? "",
    author: detail.post.author ?? "Local Control",
    category: detail.post.category ?? "",
    series: detail.post.series ?? "",
    hero_image: toBlogAssetDisplayPath(detail.post.hero_image),
    tagsText: detail.post.tags.join(", "),
    draft: detail.post.draft,
    featured: detail.post.featured,
    content: detail.content,
    pathTouched: true
  };
}

export function toBlogPostSaveRequest(draft: BlogDraft): BlogPostSaveRequest {
  return {
    original_relative_path: draft.original_relative_path,
    relative_path: draft.relative_path.trim(),
    title: draft.title.trim(),
    description: draft.description.trim(),
    pub_date: draft.pub_date,
    updated_date: draft.updated_date.trim() || null,
    author: draft.author.trim() || null,
    category: draft.category.trim() || null,
    series: draft.series.trim() || null,
    hero_image: normalizeBlogAssetPath(draft.hero_image) || null,
    tags: parseTagsText(draft.tagsText),
    draft: draft.draft,
    featured: draft.featured,
    content: draft.content
  };
}

// ─── 路径与标签工具 ───────────────────────────────────────────────────────────

export function normalizeBlogAssetPath(value: string | null | undefined): string {
  const trimmed = value?.trim() ?? "";
  if (!trimmed) return "";
  if (/^(?:[a-z][a-z\d+.-]*:|\/\/|#)/i.test(trimmed)) return trimmed;
  if (trimmed.startsWith(BLOG_ASSET_URL_PREFIX)) return trimmed;
  if (trimmed.startsWith("/")) return trimmed;
  return `${BLOG_ASSET_URL_PREFIX}${trimmed.replace(/^\/+/, "")}`;
}

export function toBlogAssetDisplayPath(value: string | null | undefined): string {
  const normalized = normalizeBlogAssetPath(value) || DEFAULT_BLOG_IMAGE_PATH;
  return normalized.startsWith(BLOG_ASSET_URL_PREFIX)
    ? normalized.slice(BLOG_ASSET_URL_PREFIX.length)
    : normalized;
}

export function parseTagsText(value: string): string[] {
  return Array.from(
    new Set(
      value
        .split(/[,，\n]+/)
        .map((item) => item.trim())
        .filter(Boolean)
    )
  );
}

export function filterTagsForCategory(
  category: string,
  tagsText: string,
  categoryTagMap: Map<string, Array<{ name: string; count: number }>>
): string {
  const normalized = category.trim();
  if (!normalized) return "";
  const allowedTags = new Set((categoryTagMap.get(normalized) ?? []).map((t) => t.name));
  return parseTagsText(tagsText).filter((t) => allowedTags.has(t)).join(", ");
}

export function slugify(value: string): string {
  const slug = value
    .trim()
    .toLowerCase()
    .replace(/[\\/:*?"<>|]+/g, "")
    .replace(/\s+/g, "-")
    .replace(/^-+|-+$/g, "");
  return slug || `post-${Date.now()}`;
}

export function matchesBlogFilters(
  post: BlogPost,
  normalizedQuery: string,
  statusFilter: BlogStatusFilter,
  categoryFilter: string,
  featuredOnly: boolean
): boolean {
  if (statusFilter === "draft" && !post.draft) return false;
  if (statusFilter === "published" && post.draft) return false;
  if (featuredOnly && !post.featured) return false;
  if (categoryFilter === "__uncategorized__" && post.category) return false;
  if (
    categoryFilter !== "all" &&
    categoryFilter !== "__uncategorized__" &&
    post.category !== categoryFilter
  ) return false;
  if (!normalizedQuery) return true;
  return [post.title, post.relative_path, post.id, post.description, post.category ?? "", post.tags.join(" ")]
    .join(" ")
    .toLowerCase()
    .includes(normalizedQuery);
}

export async function invalidateBlogQueries(queryClient: QueryClient) {
  await Promise.all([
    queryClient.invalidateQueries({ queryKey: queryKeys.blog.posts }),
    queryClient.invalidateQueries({ queryKey: queryKeys.blog.taxonomy }),
    queryClient.invalidateQueries({ queryKey: queryKeys.blog.details })
  ]);
}
