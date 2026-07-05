// ─────────────────────────────────────────────────────────────────────────────
// 站点配置加载器
//
// 为什么可以在 Astro 中使用 `fs.readFileSync`（同步文件读取）？
// ─────────────────────────────────────────────────────────────────────────────
// Astro 静态构建运行在 Node.js 环境中（不是浏览器）。
// 前置块（---）和通过 import 加载的 lib/ 文件，都在构建时由 Node.js 执行。
// 因此可以自由使用 Node.js 内置模块：`node:fs`（文件系统）、`node:path`（路径）等。
// 这些代码不会出现在浏览器端的 bundle 中。
//
// 与服务端框架（如 Express）不同，这里用同步读取（readFileSync 而非 readFile）完全可以接受，
// 因为构建是一次性的离线任务，不需要像服务器那样担心阻塞并发请求。
// ─────────────────────────────────────────────────────────────────────────────

import { existsSync, readFileSync } from "node:fs";
import {
  DEFAULT_BLOG_IMAGE_PATH,
  normalizeBlogAssetPath
} from "@lib/blog-assets";

// TypeScript interface：定义站点配置对象的"形状"（每个字段的名称和类型）。
// 这是纯类型声明，不生成任何运行时代码，只用于编译期类型检查和 IDE 补全。
export interface SiteConfig {
  site_url: string;
  site_name: string;
  site_title: string;
  site_description: string;
  hero_title: string;
  hero_subtitle: string;
  background_image: string;
  announcement: string;
  avatar_image: string;
  footer_note: string;
}

// 默认配置：当配置文件不存在或读取失败时的 fallback（回退）值。
// 这是一种防御性编程策略：无论外部配置状态如何，站点总能以合理的默认值运行。
export const defaultSiteConfig: SiteConfig = {
  site_url: "https://home.lan",
  site_name: "Poetic Notes",
  site_title: "Poetic Notes",
  site_description: "有诗意地记录生活、技术、局域网自托管和长期维护的个人博客。",
  hero_title: "有诗意地记录",
  hero_subtitle: "把生活、服务和长期维护写清楚",
  background_image: DEFAULT_BLOG_IMAGE_PATH,
  announcement: "文章草稿可以在控制台维护，发布后会自动进入博客首页。",
  avatar_image: DEFAULT_BLOG_IMAGE_PATH,
  footer_note: "Built with Astro, MDX and a local-first publishing workflow."
};

// `import.meta.url` 是当前文件的 URL（如 file:///project/src/lib/siteConfig.ts）。
// `new URL("相对路径", import.meta.url)` 利用 URL 解析把相对路径转为绝对路径，
// 这比手拼字符串更可靠，并保持 Linux 文件 URL 的解析规则一致。
const configUrl = new URL("../../../data/content/.site.json", import.meta.url);

/**
 * 加载站点配置，带 fallback 策略。
 *
 * Fallback 策略（优先级从高到低）：
 * 1. 配置文件存在且格式正确 → 使用配置文件的值（字段级 fallback，见 normalizeSiteConfig）；
 * 2. 配置文件不存在 → 直接使用 defaultSiteConfig；
 * 3. 配置文件存在但 JSON 解析失败 → 捕获异常，使用 defaultSiteConfig。
 *
 * 这样设计的原因：配置文件由union写入，初次安装时可能还没有配置文件，
 * 或者写入过程中出现格式错误，博客不应因此无法访问。
 */
export function getSiteConfig(): SiteConfig {
  if (!existsSync(configUrl)) {
    return defaultSiteConfig;
  }

  try {
    // `as Partial<SiteConfig>` 类型断言：
    // `Partial<T>` 是 TypeScript 内置工具类型，把所有字段都变成可选的。
    // 因为外部 JSON 文件可能只有部分字段，这里先按"所有字段都可能缺失"处理，
    // 由 normalizeSiteConfig 负责补全缺失字段的默认值。
    const parsed = JSON.parse(readFileSync(configUrl, "utf8")) as Partial<SiteConfig>;
    return normalizeSiteConfig(parsed);
  } catch {
    // JSON.parse 失败（格式错误）或 readFileSync 失败（权限问题等）时，
    // 静默回退到默认配置，不让构建失败。
    return defaultSiteConfig;
  }
}

/**
 * 将外部配置（可能缺失某些字段）与默认配置合并，保证返回完整的 SiteConfig。
 * 每个字段独立处理，不会因为一个字段异常就影响其他字段。
 */
function normalizeSiteConfig(config: Partial<SiteConfig>): SiteConfig {
  return {
    site_url: clean(config.site_url, defaultSiteConfig.site_url),
    site_name: clean(config.site_name, defaultSiteConfig.site_name),
    site_title: clean(config.site_title, defaultSiteConfig.site_title),
    site_description: clean(
      config.site_description,
      defaultSiteConfig.site_description
    ),
    hero_title: clean(config.hero_title, defaultSiteConfig.hero_title),
    hero_subtitle: clean(config.hero_subtitle, defaultSiteConfig.hero_subtitle),
    background_image: normalizeBlogAssetPath(
      config.background_image,
      defaultSiteConfig.background_image
    ),
    announcement: typeof config.announcement === "string"
      ? config.announcement.trim()
      : defaultSiteConfig.announcement,
    avatar_image: normalizeBlogAssetPath(
      config.avatar_image,
      defaultSiteConfig.avatar_image
    ),
    footer_note: typeof config.footer_note === "string"
      ? config.footer_note.trim()
      : defaultSiteConfig.footer_note
  };
}

/**
 * 清理字符串值：去除首尾空白，空字符串或非字符串时使用 fallback。
 * 防御外部 JSON 中出现 null、数字或空字符串等非预期值。
 */
function clean(value: unknown, fallback: string): string {
  return typeof value === "string" && value.trim() ? value.trim() : fallback;
}
