# Blog 开发说明

## 环境

- Linux；
- Node.js 版本以仓库根目录 `.node-version` 为准；
- npm 主版本以 `blog/source/package.json` 的 `engines` 为准。

## 本地启动

```bash
cd blog/source
npm ci
npm run dev
```

开发服务器监听 `127.0.0.1:4321`。Astro 配置会确保 `blog/data/content/` 和 `blog/data/files/` 存在，并把它们加入 Vite 文件监听。

## 关键源码

| 路径 | 说明 |
| --- | --- |
| `source/astro.config.mjs` | Astro、Vite、资源映射和数据目录监听 |
| `source/src/content.config.ts` | Markdown/MDX frontmatter schema |
| `source/src/lib/siteConfig.ts` | `.site.json` 加载和默认站点配置 |
| `source/src/lib/posts.ts` | 文章排序、分组、URL、相关文章 |
| `source/src/pages/` | 首页、归档、文章页和 404 |
| `source/scripts/` | 复制资源与发布静态产物 |

## 构建验证

```bash
cd blog/source
npm run build
npm audit --audit-level=high
```

构建流程：

1. Astro 读取 `../data/content`。
2. 输出到 `dist.next/`。
3. `copy-blog-assets.mjs` 把 `blog/data/files/` 中允许公开的文件复制到 `dist.next/blog-assets/`。
4. `publish-static.mjs` 把 `dist.next/` 原子切换为 `dist/`。

## 常见问题

| 现象 | 检查 |
| --- | --- |
| 构建提示没有文章 | `blog/data/content/` 是否已有导出内容；空站点可正常构建 |
| frontmatter 校验失败 | 对照 [data-contract.md](data-contract.md) 检查字段和日期格式 |
| 图片不显示 | 资源是否在 `blog/data/files/`，引用路径是否以 `/blog-assets/` 为前缀 |
| 页面 SEO 地址不对 | `.site.json` 的 `site_url` 或构建环境 `PUBLIC_SITE_URL` |
