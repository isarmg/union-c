# 初学者代码阅读指南

本文不要求预先熟悉 Rust、React 或 Astro。目标是先建立项目运行模型，再按一条真实调用链阅读代码。遇到不认识的语法时，先确认“数据从哪里来、经过谁、最后写到哪里”，通常比逐行查语法更有效。

## 先记住四个程序

| 目录 | 可以把它理解成 | 主要输入 | 主要输出 |
| --- | --- | --- | --- |
| `back/back` | 管理页面 | 用户操作、`/api/*` JSON | 浏览器界面 |
| `back/union` | 总控制器 | HTTP 请求、配置、数据库数据 | API、服务操作、审计记录 |
| `back/ram` | 独立文件服务器 | HTTP/WebDAV 请求、生成配置 | 文件和访问日志 |
| `back/blog` | 静态网页生成器 | 导出的文章和资源 | 可由 Caddy 托管的 `dist/` |

“back”容易产生歧义：`back/back` 是 React 管理前端，`back/union` 才是管理 API 后端。四个目录各有自己的构建清单和依赖锁，不是一个统一的 Cargo 或 npm 工作区。

## 推荐阅读顺序

### 1. 后端如何启动

从 `back/union/src/main.rs` 开始。它只做三件事：初始化日志、调用 `startup::initialize()`、把路由交给 Axum 监听。接着阅读 `startup.rs`：

```text
读取启动配置
  -> 创建必要目录和密钥
  -> 创建或读取本地管理员
  -> 连接 PostgreSQL
  -> 执行数据库迁移
  -> 读取数据库运行配置
  -> 创建 AppState
  -> 恢复期望运行的服务
```

这里有两类配置。数据库连接和加密主密钥必须先获得，叫“启动配置”；端口、外部主机和博客设置可从数据库读取，叫“运行配置”。因此启动过程先创建 `bootstrap_settings`，连接数据库后再得到最终的 `settings`。

### 2. 一次 API 请求如何流动

以 `GET /api/blog/posts` 为例：

```text
浏览器页面
  -> back/back/src/api.ts
  -> HTTP GET /api/blog/posts
  -> back/union/src/routes/mod.rs 的全局中间件
  -> routes/blog.rs 的 handler
  -> blog/ 业务逻辑或 database/blog/ 查询
  -> PostgreSQL
  -> JSON 响应返回浏览器
```

`routes/` 负责 HTTP 细节，例如路径参数、认证和响应状态；`blog/`、`service_manager/` 等模块负责业务步骤；`database/` 只负责持久化查询。新增功能时应保持这个方向，不要让数据库模块反过来依赖路由。

### 3. 前端如何取得数据

从 `back/back/src/main.tsx` 开始。React 应用挂载后，页面组件通过 hooks 调用 `api.ts`。`api.ts` 集中处理：

- 15 秒请求超时；
- 会话 token 和 Cookie；
- 非只读请求的 CSRF 请求头；
- HTTP 错误到可读错误的转换；
- JSON 响应类型。

接口字段由 `back/back/src/types.ts` 描述。修改后端请求或响应结构时，必须同步检查对应 Rust `domain/` 类型和这里的 TypeScript 类型。

### 4. 数据库结构如何建立

阅读 `back/union/src/database/mod.rs` 的 `migrate`，再配合 [PostgreSQL 文档](04-database.md) 阅读 `migrations/0001_baseline.sql`。不要修改已经执行过的迁移，即使只改注释也会改变校验和。

普通数据库操作按业务拆在 `database/settings.rs`、`database/blog/`、`database/audit/` 等文件。看到 `&DbPool` 可以理解为“从共享连接池借一个数据库连接”；看到事务则表示多条操作必须一起成功或一起失败。

### 5. 博客如何发布

文章的管理源是 PostgreSQL，不是 `data/blog/content/`。发布时，Union 先把数据库内容导出成 Astro 能读取的文件，再构建到临时目录，验证成功后切换成正式 `dist/`。因此：

- 手工修改导出目录可能在下次导出时丢失；
- 构建失败不会主动破坏上一次成功产物；
- 备份必须包含数据库和博客资源，不能只备份 `dist/`。

## 常见 Rust 语法对照

| 写法 | 在本项目中的含义 |
| --- | --- |
| `Result<T, E>` / `anyhow::Result<T>` | 操作可能失败；`Ok` 是成功值，`Err` 是错误。 |
| `?` | 失败时立刻把错误返回给调用者，成功时取出内部值。 |
| `async fn` / `.await` | 函数会等待数据库、网络或进程操作，但不会阻塞整个异步运行时。 |
| `Arc<T>` | 多个请求共享同一份数据，不复制完整对象。 |
| `Mutex<T>` | 同一时刻只允许一个任务修改数据。 |
| `RwLock<T>` | 可同时有多个读取者，但写入时独占。 |
| `Option<T>` | 值可能存在（`Some`），也可能不存在（`None`）。 |
| `impl` | 为结构体实现方法。 |
| `mod` / `pub mod` | 声明模块；`pub` 表示其他模块可以访问。 |

锁不能随意删除。它们通常保护“读取旧值、修改、保存新值”这一整段流程，而不仅是单个字段。缩小锁范围前应先证明并发请求不会互相覆盖。

## 修改功能时的同步检查

| 修改内容 | 通常还要检查 |
| --- | --- |
| API 路径或 JSON 字段 | `routes/`、`domain/`、前端 `api.ts` 和 `types.ts`、`07-api.md` |
| 数据库字段 | 新迁移、所有相关 SQL、Rust 记录类型、集成测试、`04-database.md` |
| 运行配置 | `app_config/`、设置读写、设置页面、环境模板、`03-configuration.md` |
| 博客 frontmatter | 数据库模型、导出逻辑、Astro content schema、页面组件 |
| 文件路径 | 路径校验、systemd 写权限、备份脚本、部署文档 |
| 外部凭据 | 加密、脱敏、日志、API 返回结构、安全文档 |

## 开始修改前后的最小流程

修改前先运行 `git status --short`，避免覆盖已有工作。尽量一次只改一条调用链，并为失败路径补测试。完成后至少运行受影响子项目的格式检查和测试；完整命令见 [开发与验证](02-development.md)。

阅读代码时可用以下搜索建立调用关系：

```bash
# 查找函数在哪里定义、在哪里调用
rg -n "function_name" back/union/src

# 查找某个 API 路径的前后端使用位置
rg -n 'api/blog/posts' back/back/src back/union/src

# 查找某张表的所有 SQL
rg -n 'blog_posts' back/union/src back/union/migrations
```
