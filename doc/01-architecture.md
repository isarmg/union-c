# 项目结构与架构

## 顶层结构

```text
.
├── back/
│   ├── back/        React 管理界面
│   ├── union/        Rust 管理后端
│   ├── ram/        Rust 文件服务（ram 内核）
│   └── blog/        Astro 静态博客
├── config/            Caddy、systemd、tmpfiles、logrotate 配置
├── doc/               项目文档
├── scripts/           Linux 运维脚本
├── .env.production.example
├── .node-version
└── rust-toolchain.toml
```

四个子项目在构建层面彼此独立，各自拥有 manifest、锁文件和产物；运行时则由 `union` 编排 `ram` 和博客构建：

| 目录 | 技术 | 独立入口 | 产物 |
| --- | --- | --- | --- |
| `back/back` | React、TypeScript、Vite | `package.json`、`package-lock.json` | `dist/` |
| `back/union` | Rust、Axum、SQLx | `Cargo.toml`、`Cargo.lock` | `target/release/union` |
| `back/ram` | Rust、Hyper | `Cargo.toml`、`Cargo.lock` | `target/release/ram` |
| `back/blog` | Astro、TypeScript | `package.json`、`package-lock.json` | `dist/` |

根目录不是 Cargo workspace。`union` 与 `ram` 分别维护依赖锁、测试和发布流程；两个 Web 项目也分别维护 `node_modules`。

## 组件职责

### `back/back`：管理端

管理端单页应用，只通过同源 `/api/*` 与 `union` 通信。它负责登录、服务总览、博客编辑、ram 权限、Sunshine 主机和 Proxmox VE 管理，不直接连接 PostgreSQL 或外部服务。

### `back/union`：控制面

系统控制面，负责：

- 认证、会话、CSRF 和审计；
- PostgreSQL 基线建表与配置持久化；
- 博客文章、分类、标签和首页配置管理；
- 把博客数据原子导出到 `data/blog/content/`；
- 调用 `blog` 的构建命令并发布静态产物；
- 生成 ram 私有配置，启停并探测 ram 子进程；
- 代理 Sunshine 和 Proxmox VE API；
- 提供健康检查、资源状态和 SSE 实时事件。

`union/src` 内部按以下边界组织：

| 层次 | 主要模块 | 职责 |
| --- | --- | --- |
| 进程入口 | `main.rs`、`lib.rs`、`startup.rs` | 薄二进制负责日志与监听；库负责可测试的应用组装、初始化、后台维护和服务恢复 |
| HTTP 接入 | `routes/` | 各业务域自行声明路由；大型代理按主机、VM/容器和代理操作继续拆分；`access_control.rs` 统一处理认证、CSRF 和数据库可用性 |
| 业务服务 | `blog/`、`service_manager/`、`ram_auth/`、`ram_instances.rs` | 博客文章/分类/构建/导出、进程生命周期、RAM 协议客户端、权限规则和远程实例管理 |
| 外部适配 | `sunshine.rs`、`proxmox.rs`、`wol.rs`、`http_client.rs`、`network.rs` | 复用连接池，统一地址处理，调用外部 API 或网络协议 |
| 持久化 | `database/`、`migrations/` | 按版本迁移，以及设置、审计、服务账号、后台任务和博客等事务性数据访问 |
| 共享模型与状态 | `domain/`、`state.rs`、`app_config/`、`error.rs` | 按业务域组织的 API 合同、分组并发状态、配置和统一错误映射 |

依赖方向保持为 HTTP 接入调用业务服务，业务服务通过 `database/` 或外部适配访问资源。目录模块的 `mod.rs` 只承担类型/门面和重导出，不堆叠业务实现；新增端点应在对应业务路由模块注册。

### `back/ram`：文件数据面

独立文件服务程序。它只处理文件浏览、上传、下载、认证、WebDAV、压缩和 HTTP 日志。`union` 不复制这些能力，而是生成配置并管理该进程。

### `back/blog`：静态站点生成器

只读的静态站点构建器。文章来自 `data/blog/content/`，图片和附件来自 `data/blog/files/`。生产环境由 Caddy 直接托管构建后的 `dist/`。

## 主要数据流

```text
浏览器
  ├─ 管理站点 ──> Caddy ──> back 静态文件
  │                         └─ /api/* ──> union ──> PostgreSQL
  ├─ 博客站点 ──> Caddy ──> blog/dist
  └─ 文件站点 ──> Caddy ──> ram

union ──导出文章──> data/blog/content ──> blog 构建
union ──生成配置/启停──> ram ──读写──> data/ram/files
union ──HTTPS API──> Sunshine / Proxmox VE
```

## 状态所有权

| 状态 | 唯一管理源 | 派生物 |
| --- | --- | --- |
| Union 管理员凭据 | `data/union-config.json`（0600） | 内存 bcrypt 校验数据 |
| Union 登录会话 | 进程内存 | 重启后全部失效 |
| 运行配置和外部凭据 | PostgreSQL 加密字段 | 内存配置、ram 私有 YAML |
| 博客文章和分类标签 | PostgreSQL | `data/blog/content/*` |
| 博客静态资源 | `data/blog/files/` | `back/blog/dist/blog-assets/` |
| 文件服务内容 | `data/ram/files/` | 无 |
| 管理台和博客页面 | 源代码 | 各自 `dist/` |

数据库与 `data/` 必须一起备份。派生内容可以重建，但不能用派生内容替代数据库备份。

## 启动与发布生命周期

1. `union` 读取启动环境、初始化密钥，并在本地私有配置不存在时创建首个管理员。
2. 连接 PostgreSQL、执行基线建表并加载运行配置。
3. 校验生产约束并创建运行目录。
4. 恢复本机托管的 `ram` 期望状态；远程 RAM 主机仅执行健康探测，不管理其进程。
5. Caddy 从管理端和博客的 `dist/` 提供静态文件，把 API 与文件请求反向代理到回环端口。

博客发布不是直接修改线上 `dist/`：PostgreSQL 内容先导出到 `data/blog/content/`，Astro 构建到 `dist.next`，验证成功后再切换为 `dist`。

## 故障边界

- PostgreSQL 不可用时，`union` 无法就绪，管理 API 与编排能力不可用。
- `union` 停止不会影响 Caddy 已有的博客静态页面，但管理 API 不可用。
- 单台远程 RAM 主机不可达不会阻止 `union` 提供管理 API。
- 博客构建失败会保留最近一次成功发布的 `dist`，错误记录到构建日志。

## Linux 边界

`union` 和 `ram` 在非 Linux 目标上会直接编译失败。实现使用 Linux 信号、Unix 权限和 Unix socket，不保留其他操作系统的备用分支。CI 也只在 Ubuntu 上验证。
