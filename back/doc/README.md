# Back 管理界面文档

`back/` 是 React 管理界面项目，负责登录、总览、博客编辑、文件服务账号、Sunshine 主机和 Proxmox VE 管理。它不直接访问 PostgreSQL、ram、Sunshine 或 Proxmox VE，所有业务操作都通过 Union API 完成。

## 目录职责

| 子目录 | 内容 |
| --- | --- |
| `source/` | Vite、React、TypeScript 源码、样式和构建脚本 |
| `data/` | 管理界面预留运行数据目录，当前只保留 `.gitkeep` |
| `doc/` | 管理界面开发、部署和边界说明 |

## 文档

| 目标 | 文档 |
| --- | --- |
| 本地开发、构建、联调 | [development.md](development.md) |
| 独立 Linux 部署 | [deployment.md](deployment.md) |
| Union API 约定 | [../../union/doc/api.md](../../union/doc/api.md) |
| 四机拓扑 | [../../union/doc/four-linux-deployment.md](../../union/doc/four-linux-deployment.md) |

## 运行模型

- 浏览器加载 `back/source/dist` 中的静态文件。
- 页面请求同源 `/api/*`。
- 开发环境由 Vite 把 `/api/*` 代理到 `http://127.0.0.1:8080`。
- 生产环境推荐由 Back 主机本机 Caddy 把 `/api/*` 反向代理到 Union 主机。
- 会话 Cookie 由 Union 设置，当前安全策略要求保持同源访问。

## 常用命令

```bash
cd back/source
npm ci
npm run dev
npm run build
```

构建产物在 `back/source/dist/`。`node_modules/`、`dist/`、`dist.next/` 和 `dist.previous/` 都是生成内容，不提交。
