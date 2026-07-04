# Union 文档

本目录是项目详细 Markdown 文档的统一入口。项目仅支持 Linux 部署，不维护 Windows、macOS 或其他系统的运行适配。

根目录的 [`README.md`](../README.md) 提供项目概览；`scripts/deploy-linux.sh` 提供 Linux 部署检查、配置、构建、安装和验证命令。本文档以仓库内的源码和配置为准。

## 按任务查找

| 目标 | 从这里开始 | 后续文档 |
| --- | --- | --- |
| 第一次本地运行 | [开发与验证](02-development.md) | [配置说明](03-configuration.md) |
| 第一次阅读源码 | [初学者代码阅读指南](00-code-reading-guide.md) | [项目结构与架构](01-architecture.md) |
| 理解组件和数据边界 | [项目结构与架构](01-architecture.md) | [PostgreSQL](04-database.md) |
| 部署新服务器 | [Linux 生产部署](05-linux-deployment.md) | [安全设计](08-security.md) |
| 更新、备份或排障 | [运行维护](06-operations.md) | [PostgreSQL](04-database.md) |
| 开发管理端/API | [HTTP API](07-api.md) | [项目结构与架构](01-architecture.md) |
| 准备提交 | [仓库维护规则](09-repository-policy.md) | [开发与验证](02-development.md) |

## 阅读顺序

1. [初学者代码阅读指南](00-code-reading-guide.md)：从入口跟踪一次请求和一次数据修改。
2. [项目结构与架构](01-architecture.md)：理解四个独立子项目及数据流。
3. [开发与验证](02-development.md)：准备环境、启动服务、执行测试。
4. [配置说明](03-configuration.md)：环境变量、运行配置和目录约定。
5. [PostgreSQL](04-database.md)：基线结构、初始化、清空、备份和恢复。
6. [Linux 生产部署](05-linux-deployment.md)：从新主机部署到 systemd、Caddy。
7. [运行维护](06-operations.md)：更新、监控、日志、备份和故障处理。
8. [HTTP API](07-api.md)：认证约定和路由清单。
9. [安全设计](08-security.md)：凭据、网络、文件和服务加固。
10. [仓库维护规则](09-repository-policy.md)：哪些文件允许进入版本库。

## 最小验证

在仓库根目录运行：

```bash
cargo test --manifest-path back/union/Cargo.toml --all-targets --locked
cargo test --manifest-path back/ram/Cargo.toml --all-targets --locked

(cd back/back && npm ci && npm run build)
(cd back/blog && npm ci && npm run build)
```

Rust、Node.js 和 npm 的固定版本分别记录在 `rust-toolchain.toml`、`.node-version` 和两个前端项目的 `package.json` 中。

## 重要边界

- `back/` 只放四个独立源代码项目。
- `data/` 是运行时状态，首次启动由 `union` 创建，禁止提交。
- PostgreSQL 是账号、配置、文章和审计信息的管理源。
- `blog` 只消费从 PostgreSQL 导出到 `data/blog/content/` 的内容。
- 对公网或局域网只暴露 Caddy；后端和文件服务监听回环地址。
- 数据库初始化要求目标数据库中不存在 Union 业务表。
