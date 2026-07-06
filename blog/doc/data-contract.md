# Blog 数据契约

`blog/source` 读取两个输入目录：

| 路径 | 说明 |
| --- | --- |
| `blog/data/content/` | Markdown/MDX 文章、`.site.json`、`.taxonomy.json` |
| `blog/data/files/` | 图片、附件和默认封面图等公开资源 |

## 文章 frontmatter

文章文件扩展名为 `.md` 或 `.mdx`。必填字段：

`.mdx` 只适合由受信任管理员维护的内容。它不应作为低权限用户或匿名用户投稿格式；
如果需要接收不可信内容，应改用纯 Markdown 并在进入数据库前做内容净化。

| 字段 | 类型 | 说明 |
| --- | --- | --- |
| `title` | string | 文章标题 |
| `description` | string | 摘要和 SEO 描述 |
| `pubDate` | date | 发布日期，建议 `YYYY-MM-DD` |

常用可选字段：

| 字段 | 类型 | 默认值 |
| --- | --- | --- |
| `updatedDate` | date | 无 |
| `author` | string | `Local Control` |
| `category` | string | `运维笔记` |
| `tags` | string[] | `[]` |
| `featured` | boolean | `false` |
| `series` | string | 无 |
| `draft` | boolean | `false` |
| `heroImage` | string | 默认图 |

`draft: true` 的文章不会出现在前台页面。

## 站点配置

`.site.json` 由 Union 导出。Blog 构建时读取它，并对缺失字段使用默认值。常见字段包括：

- `site_url`
- `site_name`
- `site_title`
- `site_description`
- `hero_title`
- `hero_subtitle`
- `background_image`
- `announcement`
- `avatar_image`
- `footer_note`

图片字段可以写 `/blog-assets/...`，也可以写相对资源名；构建时会规范化到 `/blog-assets/`。

## 资源目录

`blog/data/files/` 会复制到 `dist/blog-assets/`。构建脚本只复制常见公开扩展名，例如光栅图片、PDF、文本和 zip 文件；隐藏文件、符号链接和 SVG 不会发布。

文章和站点配置中引用资源时，推荐使用：

```text
/blog-assets/images/example.webp
```

## 同步要求

四机部署时，`content/` 和 `files/` 必须作为同一发布批次同步。同步过程中不要让 Caddy 指向半成品目录；先同步到临时目录，验证后再切换。
