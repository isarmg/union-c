# Union 控制面文档

`union/` 是 Rust 管理 API 和运行时编排项目。源码在 `union/source/`，本地私有配置和控制面运行数据在 `union/data/`，本文档目录只记录控制面、部署和跨项目协作规则。

## 按任务查找

| 目标 | 从这里开始 | 后续文档 |
| --- | --- | --- |
| 第一次本地运行 | [开发与验证](development.md) | [配置说明](configuration.md) |
| 第一次阅读源码 | [代码阅读指南](code-reading-guide.md) | [项目结构与架构](architecture.md) |
| 理解组件和数据边界 | [项目结构与架构](architecture.md) | [PostgreSQL](database.md) |
| 部署新服务器 | [Linux 生产部署](linux-deployment.md) | [安全设计](security.md) |
| 四机独立部署 | [四机独立部署](four-linux-deployment.md) | [Linux 生产部署](linux-deployment.md) |
| 更新、备份或排障 | [运行维护](operations.md) | [PostgreSQL](database.md) |
| 开发管理端/API | [HTTP API](api.md) | [管理界面文档](../../back/doc/README.md) |
| 准备提交 | [仓库维护规则](repository-policy.md) | [开发与验证](development.md) |

## 相关项目文档

- [back 管理界面](../../back/doc/README.md)：开发见 [development.md](../../back/doc/development.md)，独立部署见 [deployment.md](../../back/doc/deployment.md)。
- [ram 文件服务](../../ram/doc/README.md)：开发见 [development.md](../../ram/doc/development.md)，独立部署见 [deployment.md](../../ram/doc/deployment.md)。
- [blog 静态博客](../../blog/doc/README.md)：开发见 [development.md](../../blog/doc/development.md)，数据契约见 [data-contract.md](../../blog/doc/data-contract.md)，发布见 [publishing.md](../../blog/doc/publishing.md)。

## 最小验证

在仓库根目录运行：

```bash
cargo test --manifest-path union/source/Cargo.toml --all-targets --locked
cargo test --manifest-path ram/source/Cargo.toml --all-targets --locked

(cd back/source && npm ci && npm run build)
(cd blog/source && npm ci && npm run build)
```

Rust、Node.js 和 npm 的固定版本分别记录在 `rust-toolchain.toml`、`.node-version` 和两个前端项目的 `package.json` 中。

## 重要边界

- 四个顶层项目分别是 `back/`、`union/`、`ram/`、`blog/`。
- 每个项目内部固定包含 `source/`、`data/`、`doc/`。
- 各项目 `data/` 内容是运行时状态，禁止提交；只保留 `.gitkeep`。
- PostgreSQL 是账号、配置、文章和审计信息的管理源。
- `blog` 只消费从 PostgreSQL 导出到 `blog/data/content/` 的内容。
- 对公网或局域网只暴露 Caddy；后端和文件服务监听回环地址。
- 数据库初始化要求目标数据库中不存在 Union 业务表。
