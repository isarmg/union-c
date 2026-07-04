/*
 * Astro 内容集合（Content Collections）配置。
 *
 * ─── 什么是内容集合？ ────────────────────────────────────────────────────────
 * Astro 的内容集合是管理博客文章（Markdown/MDX）的官方方案。
 * 核心思路：把一批结构相似的内容文件（如所有文章）定义为一个"集合"，
 * 并用 schema（结构描述）约束每个文件的 frontmatter 字段。
 *
 * 好处：
 * - 类型安全：TypeScript 知道每篇文章有哪些字段，IDE 可以自动补全；
 * - 构建时校验：文章 frontmatter 缺少必填字段或类型不对时，构建直接报错，
 *   而不是等到页面渲染时才出现奇怪的 bug；
 * - 统一 API：所有页面都通过 `getCollection("posts")` 等 API 访问内容，
 *   不需要自己写文件读取逻辑。
 *
 * ─── 什么是 Zod schema？ ──────────────────────────────────────────────────────
 * Zod 是一个 TypeScript 数据验证库，用来描述数据的形状（shape）并在运行时验证它。
 * `z.object({...})` 定义一个对象的结构，`z.string()`、`z.boolean()` 等定义字段类型。
 * `.optional()` 表示该字段可以不存在，`.default(value)` 表示缺失时使用默认值。
 * `.coerce.date()` 表示自动将字符串（如 "2024-01-01"）转换为 Date 对象。
 *
 * 示例：一篇文章的 frontmatter 如果写了 `pubDate: "not-a-date"`，
 * Zod 会在构建时报错 "Invalid date"，提醒作者修正。
 */
import { defineCollection, z } from "astro:content";
import { glob } from "astro/loaders";

// `defineCollection` 是 Astro 提供的辅助函数，接受集合的加载器（loader）和 schema，
// 返回一个类型化的集合描述对象。
const posts = defineCollection({
  // `glob` 加载器：扫描指定目录下的所有匹配文件作为集合内容。
  // pattern: 匹配 Markdown 和 MDX 文件（`**` 表示递归匹配子目录）。
  // base: 内容目录的路径（相对于这个配置文件）。
  //   这里指向 data/blog/content，是union从数据库导出文章的目录。
  loader: glob({
    // 博客内容由union从 PostgreSQL 导出到 data/blog/content，Astro 只消费导出物。
    pattern: "**/*.{md,mdx}",
    base: "../../data/blog/content"
  }),
  // schema 定义每篇文章 frontmatter 的结构和验证规则。
  // frontmatter 是 Markdown 文件最顶部 `---` 之间的 YAML 元数据，例如：
  // ---
  // title: "我的第一篇文章"
  // pubDate: 2024-01-01
  // tags: ["运维", "Linux"]
  // ---
  schema: z.object({
    title: z.string(),                             // 必填：文章标题
    description: z.string(),                       // 必填：文章简介（用于 SEO 和卡片展示）
    pubDate: z.coerce.date(),                      // 必填：发布日期，字符串会自动转为 Date 对象
    updatedDate: z.coerce.date().optional(),       // 可选：最后更新日期
    author: z.string().default("Local Control"),   // 可选，默认值 "Local Control"
    category: z.string().default("运维笔记"),       // 可选，默认分类
    tags: z.array(z.string()).default([]),          // 可选，标签数组，默认空数组
    featured: z.boolean().default(false),          // 可选，是否为精选文章
    series: z.string().optional(),                 // 可选，所属系列名称
    draft: z.boolean().default(false),             // 可选，是否为草稿（草稿不显示在首页）
    heroImage: z.string().optional()              // 可选，文章封面图路径
  })
});

// `collections` 是固定的导出名称，Astro 框架会识别并注册所有在此声明的集合。
// 键名（"posts"）是集合名，在代码中通过 getCollection("posts") 引用。
export const collections = { posts };
