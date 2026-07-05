# 四机独立部署

本模式把四个子项目部署到四台互相独立的 Linux 主机。四台主机不共享本地文件系统，每台主机只部署自己的项目目录，并维护自己的 `data/`。

## 主机分工

| 主机 | 项目 | 运行内容 | 对外入口 |
| --- | --- | --- | --- |
| 管理界面主机 | `back/` | `back/source/dist` 静态文件，Caddy `/api/*` 反代 | HTTPS 管理域名 |
| 控制面主机 | `union/` | `union` Rust API、PostgreSQL 连接、审计、外部服务代理 | 内网 API 地址 |
| 文件服务主机 | `ram/` | `ram` 文件服务进程和文件数据 | HTTPS 文件域名 |
| 博客主机 | `blog/` | Astro 构建任务和 `blog/source/dist` 静态站点 | HTTPS 博客域名 |

## 网络关系

```text
浏览器 -> back 主机 Caddy -> /api/* -> union 主机
浏览器 -> blog 主机 Caddy -> blog/source/dist
浏览器 -> ram 主机 Caddy -> ram 进程
union 主机 -> PostgreSQL
union 主机 -> ram 主机管理接口
blog 主机 <- 显式同步 blog/data/content 和 blog/data/files
```

管理端推荐保持同源访问：`back` 主机托管管理界面，并把 `/api/*` 反向代理到 Union。不要让浏览器直接跨域访问 Union API；当前会话 Cookie 使用 Strict SameSite。

## Union 主机

Union 主机只需要 `union/` 项目、生产环境文件和数据库连接。四机部署时保持：

```bash
UNION_REQUIRE_LOCAL_STATIC_ARTIFACTS=0
```

这样 Union 启动时不会要求本机存在 `back/source/dist` 或 `blog/source/dist`。

Union 仍负责：

- 本地管理员认证和会话；
- PostgreSQL 迁移、配置、博客内容和审计；
- Sunshine/Proxmox 代理；
- 远程 RAM 实例登记和认证规则同步；
- 导出博客内容到配置的 `blog_export_dir`。

如果 Union 主机不负责博客构建，管理台中的“构建博客”按钮会按 Union 当前本地 `blog.work_dir` 配置执行；四机部署应把正式构建放在 Blog 主机的发布流程中。

## Back 主机

构建：

```bash
cd back/source
npm ci
npm run build
```

Caddy 示例：

```caddyfile
admin.example.com {
    root * /opt/union/back/source/dist
    header Cache-Control "no-store"

    handle /api/* {
        reverse_proxy https://union-api.internal {
            transport http {
                dial_timeout 3s
                response_header_timeout 30s
            }
        }
    }

    handle {
        try_files {path} /index.html
        file_server
    }
}
```

验收：

```bash
curl --fail https://admin.example.com/
curl --fail https://admin.example.com/api/health
```

## Ram 主机

RAM 可完全独立运行，也可登记为 Union 的远程 RAM 实例。生产环境远程 RAM 必须使用 HTTPS 并校验证书。

部署步骤见 [../../ram/doc/deployment.md](../../ram/doc/deployment.md)。登记到 Union 后，Union 会通过 `__ram__/admin/auth` 更新远程认证规则。

验收：

```bash
curl --fail https://files.example.com/__ram__/health
```

## Blog 主机

Blog 主机只需要 `blog/` 项目。构建输入：

- `blog/data/content/`
- `blog/data/files/`

这两个目录必须通过显式发布流程从 Union 管理源同步到 Blog 主机。同步可以使用 `rsync`、备份恢复、对象存储拉取或后续专用发布脚本，但必须保证内容和资源来自同一批次。

构建：

```bash
cd blog/source
npm ci
npm run build
```

Caddy 托管 `blog/source/dist`。更多数据契约见 [../../blog/doc/data-contract.md](../../blog/doc/data-contract.md)，发布流程见 [../../blog/doc/publishing.md](../../blog/doc/publishing.md)。

## 备份边界

四机部署至少备份：

- PostgreSQL；
- `union/data/union-config.json` 和生产环境文件；
- `blog/data/files/`；
- `ram/data/files/`；
- 当前已发布的 `back/source/dist/` 和 `blog/source/dist/`，或能复现它们的源码与锁文件。

备份恢复必须使用同一时间点的数据，避免数据库文章引用不存在的博客资源，或 RAM 文件权限与实际文件不一致。

## 单机兼容

如果四个项目仍部署在同一台 Linux，可设置：

```bash
UNION_REQUIRE_LOCAL_STATIC_ARTIFACTS=1
```

这样 Union 会在生产启动时检查本机 `back/source/dist/index.html` 和 `blog/source/dist/index.html` 是否存在。
