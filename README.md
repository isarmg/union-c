# Union

Union 是一个仅面向 Linux 的自托管基础设施控制面：用一个管理端统一管理静态博客、文件服务、Sunshine 主机和 Proxmox VE。仓库包含四个构建独立、运行时协作的子项目，每个项目都有固定的 `source/`、`data/` 和 `doc/`：

- `back/`：React 管理界面；
- `union/`：Rust 管理 API、认证、PostgreSQL 与服务编排；
- `ram/`：Rust 文件服务；
- `blog/`：Astro 静态博客。

运行时数据写入各项目自己的 `data/`，内容不受 Git 管理。Union 管理员凭据保存在 `union/data/` 的本地私有配置中，业务配置、文章和审计数据以 PostgreSQL 为管理源；未配置数据库时管理端仍可启动并从控制台完成配置。生产环境通过 Caddy 暴露 HTTPS，`union` 和 `ram` 只监听本机回环地址。

## 生产部署入口

前提：Linux、PostgreSQL、Caddy、仓库指定版本的 Rust/Node.js，以及可用的 `age`。部署脚本不会创建数据库，也不会修改 Caddy 主配置。

```bash
./scripts/deploy-linux.sh check
sudo ./scripts/deploy-linux.sh configure
sudo ./scripts/deploy-linux.sh install
sudo ./scripts/deploy-linux.sh start
```

部署前请先创建 `union` PostgreSQL 角色和空数据库。初始化流程要求目标数据库中不存在 Union 业务表。

四个子项目可以分别部署在四台互相独立的 Linux 主机；拓扑和边界见 [四机独立部署](union/doc/four-linux-deployment.md)。

## 文档入口

- [union/doc/README.md](union/doc/README.md)：控制面、API、数据库、部署和运维。
- [back/doc/README.md](back/doc/README.md)：管理界面开发与构建。
- [ram/doc/README.md](ram/doc/README.md)：文件服务开发与运行边界。
- [blog/doc/README.md](blog/doc/README.md)：静态博客开发、数据输入和发布。
