# 开发与验证

## 环境要求

- Linux；
- Rust 版本以 `rust-toolchain.toml` 为准；
- Node.js 版本以 `.node-version` 为准，npm 主版本以 `package.json` 的 `engines` 为准；
- PostgreSQL 15 或更高版本；
- `npm`、`cargo`、`psql` 可从 `PATH` 找到。

不要提交 `target/`、`node_modules/`、`dist/`、`.astro/` 或 `data/`。

## 初始化 PostgreSQL

以下示例创建本地开发角色和数据库：

```bash
sudo -u postgres psql <<'SQL'
CREATE ROLE union LOGIN PASSWORD 'replace-with-a-local-password';
CREATE DATABASE union OWNER union;
SQL
```

设置连接串：

```bash
# 可选：也可以先启动 union，再从控制台“设置”页配置数据库。
export UNION_DATABASE_URL='postgresql://union:replace-with-a-local-password@127.0.0.1:5432/union'
```

后续所有 Rust 命令都应从仓库根目录执行。应用的相对路径以当前工作目录解析，从其他目录启动会把 `data/` 写到错误位置。

开发模式未提供 `UNION_SECRET_KEY` 时，`union` 会创建权限为 `0600` 的 `data/union.secret`。此文件只能用于本机开发，不能提交或复制到不受信任的位置。

## 安装依赖

四个项目独立安装和解析依赖：

```bash
cargo fetch --manifest-path back/union/Cargo.toml --locked
cargo fetch --manifest-path back/ram/Cargo.toml --locked

(cd back/back && npm ci)
(cd back/blog && npm ci)
```

## 启动开发服务

先在根目录构建 ram（内部使用 ram 引擎），并把二进制目录加入 `PATH`：

```bash
cargo build --manifest-path back/ram/Cargo.toml --locked
export PATH="$PWD/back/ram/target/debug:$PATH"
```

从仓库根目录启动 `union`，确保相对路径都落在根目录的 `data/` 和 `back/`：

```bash
cargo run --manifest-path back/union/Cargo.toml --locked
```

首次启动会把管理员 bcrypt 哈希写入权限为 `0600` 的 `data/union-config.json`。开发模式若未设置初始密码，会在终端打印一次随机密码。从控制台配置数据库时会执行基线建表、写入默认运行配置并立即切换连接，无需重启。

另开终端启动两个前端：

```bash
cd back/back
npm run dev
```

```bash
cd back/blog
npm run dev
```

默认端口：

| 服务 | 地址 |
| --- | --- |
| back | `http://127.0.0.1:3000` |
| union | `http://127.0.0.1:8080` |
| blog | `http://127.0.0.1:4321` |
| ram（由 union 按需启动） | `http://127.0.0.1:5000` |

Vite 会把管理端 `/api` 请求代理到 `union`。博客开发服务器直接读取根目录的 `data/blog/content/` 与 `data/blog/files/`。

## 测试与静态检查

```bash
cargo fmt --manifest-path back/union/Cargo.toml -- --check
cargo test --manifest-path back/union/Cargo.toml --all-targets --locked
cargo clippy --manifest-path back/union/Cargo.toml --all-targets --all-features --locked -- -D warnings

cargo fmt --manifest-path back/ram/Cargo.toml -- --check
cargo test --manifest-path back/ram/Cargo.toml --all-targets --locked
cargo clippy --manifest-path back/ram/Cargo.toml --all-targets --locked -- -D warnings

(cd back/back && npm run build)
(cd back/blog && npm run build)
```

需要专用空 PostgreSQL 数据库时，可额外运行真实迁移与事务集成测试：

```bash
UNION_TEST_DATABASE_URL='postgresql://union:password@127.0.0.1:5432/union_test' \
  cargo test --manifest-path back/union/Cargo.toml --test database_migrations --locked -- --nocapture
```

该数据库只能用于测试；测试会写入和删除固定的集成测试记录。未配置变量时测试自动跳过，常规 HTTP 访问控制集成测试仍会执行。

依赖安全检查：

```bash
cargo audit --file back/union/Cargo.lock
cargo audit --file back/ram/Cargo.lock
(cd back/back && npm audit --audit-level=high)
(cd back/blog && npm audit --audit-level=high)
```

## 修改约定

- 修改 Rust 依赖时，只更新对应子项目的 `Cargo.lock`。
- 修改 Web 依赖时，用对应目录下的 npm 更新 `package-lock.json`。
- 修改 API 响应结构时，同步检查 `back/union/src/domain/` 对应业务文件与 `back/back/src/types.ts`。
- 修改博客内容结构时，同时检查数据库读写、导出 frontmatter 和 Astro content schema。
- 修改目录名或运行路径时，检查 `app_config/`、Caddy、systemd、CI、备份脚本和本目录文档。
- 目录模块的 `mod.rs` 只做门面、共享类型和重导出；业务实现放入按职责命名的子模块。
- 跨请求共享的可变状态放入 `AppState` 的 `hosts`、`blog`、`auth` 或 `ram` 分组，不直接向根状态追加零散锁。
- 新增外部 HTTP 调用时复用 `http_client`，新增主机地址处理时复用 `network`；不要在 handler 内创建新的连接池或复制域名校验。
