# Ram 开发说明

## 环境

- Linux；
- Rust 版本以仓库根目录 `rust-toolchain.toml` 为准。

## 测试与静态检查

```bash
cargo fmt --manifest-path ram/source/Cargo.toml -- --check
cargo test --manifest-path ram/source/Cargo.toml --all-targets --locked
cargo clippy --manifest-path ram/source/Cargo.toml --all-targets --locked -- -D warnings
```

## 构建

```bash
cargo build --manifest-path ram/source/Cargo.toml --locked
cargo build --manifest-path ram/source/Cargo.toml --release --locked
```

## 本地运行

最小运行：

```bash
ram/source/target/debug/ram ram/data/files --bind 127.0.0.1 --port 5000
```

常用参数：

| 参数 | 说明 |
| --- | --- |
| `--config FILE` | 读取 YAML 配置 |
| `--bind ADDR` | 监听 IP 或 Unix socket |
| `--port PORT` | TCP 端口，默认 5000 |
| `--path-prefix PATH` | 反向代理路径前缀 |
| `--auth RULES` | 账号和路径权限规则 |
| `--auth-state-file FILE` | 允许管理接口持久化运行时认证更新 |
| `--log-file FILE` | HTTP/进程日志输出文件 |

本地运行 Union 并让它管理 ram 时，需要把 `ram/source/target/debug` 加入 `PATH`，或在 Union 运行配置中使用 ram 二进制绝对路径。

## 关键源码

| 路径 | 说明 |
| --- | --- |
| `source/src/main.rs` | 参数解析、监听器和服务启动 |
| `source/src/args.rs` | CLI、环境变量和 YAML 配置合并 |
| `source/src/server.rs` | HTTP、WebDAV、文件操作和管理接口 |
| `source/src/auth.rs` | 认证规则和路径权限 |
| `source/assets/` | 内嵌目录页面资源 |

## 运行数据

默认运行数据写入 `ram/data/`，包括：

- `files/`：文件服务根目录；
- `logs/`：访问日志和进程日志；
- `ram.generated.yaml`：Union 生成的私有配置。

这些内容不提交。
