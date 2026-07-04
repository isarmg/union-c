/*
 * 博客文章工具函数。
 *
 * Astro 的内容集合负责读取 Markdown/MDX 文件；这个文件在读取结果之上封装排序、
 * 分组、URL 生成、阅读时长估算、相关文章推荐等常用逻辑。页面只调用这些函数，
 * 就不用重复处理文章数组。
 */
import type { CollectionEntry } from "astro:content";
import { getCollection } from "astro:content";
import { normalizeBlogAssetPath } from "@lib/blog-assets";
import { withBase } from "@lib/urls";

// posts 内容集合中的单篇文章类型。
export type Post = CollectionEntry<"posts">;
export const FEATURED_CATEGORY = "精选";

// 获取所有公开文章。draft=true 的草稿不会出现在前台。
export async function getAllPosts(): Promise<Post[]> {
  const posts = await getCollection("posts");
  return posts
    .filter((post) => !post.data.draft)
    .sort(
      (a, b) =>
        b.data.pubDate.valueOf() - a.data.pubDate.valueOf() ||
        a.data.title.localeCompare(b.data.title)
    );
}

// 获取首页精选文章；如果没有 featured=true 的文章，就回退到最新文章。
export async function getFeaturedPosts(limit = 3): Promise<Post[]> {
  const posts = await getAllPosts();
  const featured = posts.filter((post) => post.data.featured);
  return (featured.length ? featured : posts).slice(0, limit);
}

// 把内容集合的文件 id 转成 URL slug。
export function getPostSlug(post: Post): string {
  return post.id
    .replace(/\.(mdx?|md)$/i, "")
    .replace(/\/index$/i, "");
}

// 单篇文章的前台访问地址。
export function getPostUrl(post: Post): string {
  return withBase(`/posts/${getPostSlug(post)}/`);
}

// 粗略估算阅读分钟数。中文按两个汉字约等于一个英文单词估算。
export function getReadingMinutes(post: Post): number {
  const body = "body" in post ? post.body ?? "" : "";
  const latinWords = body.match(/[A-Za-z0-9_]+/g)?.length ?? 0;
  const cjkChars = body.match(/[\u4e00-\u9fa5]/g)?.length ?? 0;
  return Math.max(1, Math.ceil((latinWords + cjkChars / 2) / 220));
}

// 估算正文总字数，用于文章页元信息展示。
export function getWordCount(post: Post): number {
  const body = "body" in post ? post.body ?? "" : "";
  const latinWords = body.match(/[A-Za-z0-9_]+/g)?.length ?? 0;
  const cjkChars = body.match(/[\u4e00-\u9fa5]/g)?.length ?? 0;
  return latinWords + cjkChars;
}

// 按分类分组文章。
export function groupCategories(posts: Post[]) {
  const categories = new Map<string, Post[]>();

  for (const post of posts) {
    const category = post.data.category;
    const list = categories.get(category) ?? [];
    list.push(post);
    categories.set(category, list);
  }

  const featured = uniquePosts([
    ...posts.filter((post) => post.data.featured),
    ...(categories.get(FEATURED_CATEGORY) ?? [])
  ]);
  const sorted = Array.from(categories.entries())
    .filter(([category]) => category !== FEATURED_CATEGORY)
    .sort((a, b) => {
      if (b[1].length !== a[1].length) {
        return b[1].length - a[1].length;
      }
      return a[0].localeCompare(b[0]);
    });

  const featuredEntry: [string, Post[]] = [FEATURED_CATEGORY, featured];
  return featured.length ? [featuredEntry, ...sorted] : sorted;
}

function uniquePosts(posts: Post[]): Post[] {
  const seen = new Set<string>();
  return posts.filter((post) => {
    if (seen.has(post.id)) {
      return false;
    }
    seen.add(post.id);
    return true;
  });
}

// 按年份分组文章，用于归档页时间线。
export function groupPostsByYear(posts: Post[]) {
  const years = new Map<number, Post[]>();

  for (const post of posts) {
    const year = post.data.pubDate.getFullYear();
    const list = years.get(year) ?? [];
    list.push(post);
    years.set(year, list);
  }

  return Array.from(years.entries()).sort((a, b) => b[0] - a[0]);
}

// 获取当前文章在全站排序中的上一篇/下一篇。
export function getAdjacentPosts(posts: Post[], post: Post) {
  const index = posts.findIndex((item) => item.id === post.id);
  return {
    newer: index > 0 ? posts[index - 1] : undefined,
    older: index >= 0 && index < posts.length - 1 ? posts[index + 1] : undefined
  };
}

// 根据标签、分类、系列计算相关文章。
export function getRelatedPosts(posts: Post[], post: Post, limit = 3): Post[] {
  const postTags = new Set(post.data.tags);
  return posts
    .filter((item) => item.id !== post.id)
    .map((item) => ({
      post: item,
      score:
        item.data.tags.filter((tag) => postTags.has(tag)).length * 3 +
        (item.data.category === post.data.category ? 2 : 0) +
        (item.data.series && item.data.series === post.data.series ? 4 : 0)
    }))
    .filter((item) => item.score > 0)
    .sort((a, b) => {
      if (b.score !== a.score) {
        return b.score - a.score;
      }
      return b.post.data.pubDate.valueOf() - a.post.data.pubDate.valueOf();
    })
    .slice(0, limit)
    .map((item) => item.post);
}

// 文章没有设置 heroImage 时使用默认图片，避免卡片出现空白。
export function getPostImage(post: Post): string {
  return withBase(normalizeBlogAssetPath(post.data.heroImage));
}

// 统一中文日期展示格式。
export function formatDate(date: Date): string {
  return new Intl.DateTimeFormat("zh-CN", {
    year: "numeric",
    month: "2-digit",
    day: "2-digit"
  }).format(date);
}
