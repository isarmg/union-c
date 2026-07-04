# Union

Union 是一个仅面向 Linux 的自托管基础设施控制面：用一个管理端统一管理静态博客、文件服务、Sunshine 主机和 Proxmox VE。仓库包含四个构建独立、运行时协作的子项目：

- `back/back`：React 管理界面；
- `back/union`：Rust 管理 API、认证、PostgreSQL 与服务编排；
- `back/ram`：Rust 文件服务；
- `back/blog`：Astro 静态博客。

运行时数据统一写入不受 Git 管理的 `data/`。Union 管理员凭据保存在本地私有配置，业务配置、文章和审计数据以 PostgreSQL 为管理源；未配置数据库时管理端仍可启动并从控制台完成配置。生产环境通过 Caddy 暴露 HTTPS，`union` 和 `ram` 只监听本机回环地址。

## 生产部署入口

前提：Linux、PostgreSQL、Caddy、仓库指定版本的 Rust/Node.js，以及可用的 `age`。部署脚本不会创建数据库，也不会修改 Caddy 主配置。

```bash
./scripts/deploy-linux.sh check
sudo ./scripts/deploy-linux.sh configure
sudo ./scripts/deploy-linux.sh install
sudo ./scripts/deploy-linux.sh start
```

部署前请先创建 `union` PostgreSQL 角色和空数据库。初始化流程要求目标数据库中不存在 Union 业务表。

完整文档从 [doc/README.md](doc/README.md) 开始。第一次阅读源码建议先看 [初学者代码阅读指南](doc/00-code-reading-guide.md)，本地开发见 [doc/02-development.md](doc/02-development.md)，生产部署与验收见 [doc/05-linux-deployment.md](doc/05-linux-deployment.md)。
