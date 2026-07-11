# UnionC

UnionC 是从 `union` 派生的精简控制台后端。它只保留：

- 本地管理员登录、会话、修改密码和 CSRF 防护；
- PostgreSQL 引导配置、加密运行配置和审计日志；
- 系统 CPU、内存、磁盘、网络监控与 SSE 状态推送；
- 跨平台 Agent 的只读主机注册、指标快照、有限历史和查询 API；
- Sunshine 多主机配置、状态、Wake-on-LAN 和管理 API 代理。

Proxmox VE、静态博客、文件服务及其路由、模型、进程管理和数据库表均不包含在此项目中。

## 开发运行

在仓库根目录运行：

```bash
cargo run --manifest-path unionc/source/Cargo.toml
```

默认监听 `127.0.0.1:8081`。首次开发启动会生成 `unionc/data/unionc-config.json` 和开发管理员密码。生产环境应设置：

- `UNIONC_ENV=production`
- `UNIONC_BOOTSTRAP_PASSWORD`（首次启动，至少 12 个字符）
- `UNIONC_DATABASE_URL`
- 可选 `UNIONC_SERVER_BIND`、`UNIONC_SERVER_PORT`（适合容器和测试覆盖）
- `UNIONC_SECRET_KEY`（32 字节密钥的 Base64）
- `UNIONC_AGENT_ENROLLMENT_TOKEN`（开放新 Agent 注册时设置，至少 32 个非空白字符）
- 可选 `UNIONC_SECRET_KEY_ID`、`UNIONC_RETENTION_DAYS`、
  `UNIONC_TELEMETRY_RETENTION_DAYS`（默认 30 天）

UnionC 使用自己的迁移基线和配置密钥，建议使用独立的空 PostgreSQL 数据库，不要直接指向原 Union 数据库。

## 只读主机监控

- `POST /api/agent/v1/register`：部署 token + 主机私有 enrollment proof，换取每主机 token。
- `POST /api/agent/v1/report`：每主机 Bearer token 上报只读快照，512 KiB 请求上限。
- `GET /api/monitoring/hosts`、`/{id}`、`/{id}/history`：管理员会话只读查询。

同一主机只有持有本地私有 enrollment proof 才能重复注册和轮换 token；数据库只保存 proof
和 token 的 SHA-256。完成一批部署后可移除服务端 enrollment token 并重启，从而关闭新注册，
已注册主机仍可继续上报。生产模式强制 UnionC 只绑定回环地址，Agent API 应由 HTTPS 反向
代理暴露。代码中没有主机命令、配置下发、进程控制或自更新端点。
需要独立 Agent 域名和 mTLS 时，可从 `unionc/monitoring/Caddyfile.agent-api.example` 开始；
普通管理台域名继续由现有 Caddy 配置提供。

## 验证

```bash
cargo fmt --manifest-path unionc/source/Cargo.toml --all -- --check
cargo test --manifest-path unionc/source/Cargo.toml
```

数据库迁移和 Agent HTTP 端到端测试默认跳过；设置 `UNIONC_TEST_DATABASE_URL` 后启用。
