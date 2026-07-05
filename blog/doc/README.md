# Blog 静态博客文档

`blog/` 是 Astro 静态站点生成项目。它只负责读取已经导出的内容和资源，生成可由 Caddy 托管的静态站点。

## 目录职责

| 子目录 | 内容 |
| --- | --- |
| `source/` | Astro 页面、布局、组件、样式、内容集合配置和构建脚本 |
| `data/` | 构建输入、资源和构建日志 |
| `doc/` | 博客开发、数据契约和发布说明 |

## 文档

| 目标 | 文档 |
| --- | --- |
| 本地开发、构建 | [development.md](development.md) |
| 数据目录和 frontmatter 契约 | [data-contract.md](data-contract.md) |
| 发布流程 | [publishing.md](publishing.md) |
| 四机拓扑 | [../../union/doc/four-linux-deployment.md](../../union/doc/four-linux-deployment.md) |

## 运行模型

- `blog/source` 只读 `blog/data/content/` 和 `blog/data/files/`。
- 文章、分类、标签和首页配置的管理源是 PostgreSQL。
- Union 把数据库内容导出为 Astro 可读文件。
- Blog 主机执行 Astro 构建并托管 `blog/source/dist/`。

不要把 `blog/data/content/` 当成最终管理源。该目录是导出物，可被下一次导出覆盖。
