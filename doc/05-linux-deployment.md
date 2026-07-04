# Linux 生产部署

本文以 systemd、PostgreSQL 和 Caddy 的常规 Linux 主机为目标。部署根目录为 `/opt/union`，服务账号为 `union`。

## 自动化工具

推荐用部署工具完成检查、环境文件生成、测试、构建和 systemd 安装。以下命令在源码目录执行；`configure`、`install` 和 `start` 需要 root：

```bash
./scripts/deploy-linux.sh check
sudo ./scripts/deploy-linux.sh configure
sudo ./scripts/deploy-linux.sh install
sudo ./scripts/deploy-linux.sh start
sudo ./scripts/deploy-linux.sh verify
```

也可用 `sudo ./scripts/deploy-linux.sh --start install` 合并最后两步。`--dry-run` 只展示系统变更。可通过 `DEPLOY_ROOT`、`SERVICE_USER`、`SERVICE_GROUP` 和 `ENV_FILE` 覆盖默认安装位置；使用 `sudo` 传入覆盖值时需显式保留对应环境变量。工具不会创建 PostgreSQL 角色、修改 Caddy 主配置或猜测域名，这三项仍需显式完成。

`install` 会复制干净源码到部署目录、执行测试和构建、安装二进制及系统配置，并清理 Rust 构建目录和管理端 `node_modules`。它会保留博客 `node_modules`，因为运行时博客发布仍需执行 Astro 构建。

下面保留等价的手工流程，便于审计和故障处理。

## 1. 安装系统依赖

安装 PostgreSQL、Caddy、Rust 构建工具、Node.js/npm、age 和常用证书包。Node.js 与 Rust 版本必须匹配仓库固定版本。

创建服务账号和目录：

```bash
sudo useradd --system --home /opt/union --shell /usr/sbin/nologin union
sudo install -d -o union -g union -m 0750 /opt/union
sudo install -d -o root -g union -m 0750 /etc/union
```

## 2. 创建数据库

```bash
sudo -u postgres psql <<'SQL'
CREATE ROLE union LOGIN PASSWORD 'replace-with-a-long-random-password';
CREATE DATABASE union OWNER union;
SQL
```

限制 PostgreSQL 只监听需要的本地地址，并在 `pg_hba.conf` 中只允许应用角色访问应用库。

## 3. 安装源代码

把仓库内容同步到 `/opt/union`，排除：

- `.git/`；
- `target/`；
- `node_modules/`；
- `dist/`；
- `.astro/`；
- `data/`；
- 编辑器和代理元数据。

源码可由 root 管理，运行时可写目录只授予服务账号。

## 4. 构建四个子项目

在部署根目录执行：

```bash
cargo build --release --locked --manifest-path back/union/Cargo.toml
cargo build --release --locked --manifest-path back/ram/Cargo.toml

(cd back/back && npm ci && npm run build)
(cd back/blog && npm ci && npm run build)
```

安装二进制：

```bash
sudo install -d -o root -g root -m 0755 /opt/union/bin
sudo install -o root -g root -m 0755 \
  /opt/union/back/union/target/release/union \
  /opt/union/bin/union
sudo install -o root -g root -m 0755 \
  /opt/union/back/ram/target/release/ram \
  /opt/union/bin/ram
```

构建验证完成后，可以删除两个 `target/`；生产运行只需要安装后的二进制。

## 5. 配置环境

复制 `.env.production.example` 到 `/etc/union/union.env`，替换所有占位值：

```bash
sudo install -o root -g union -m 0640 \
  .env.production.example /etc/union/union.env
```

模板手工安装后的基线是 `root:union 0640`。若改为 `0600`，必须让服务账号成为文件所有者，否则 systemd 无法读取。首次成功创建管理员后，删除 `UNION_BOOTSTRAP_PASSWORD` 并重启服务。

## 6. 安装系统配置

```bash
sudo install -o root -g root -m 0644 config/systemd/union.service \
  /etc/systemd/system/union.service
sudo install -o root -g root -m 0644 config/tmpfiles-union.conf \
  /etc/tmpfiles.d/union.conf
sudo install -o root -g root -m 0644 config/logrotate-union \
  /etc/logrotate.d/union

sudo systemd-tmpfiles --create /etc/tmpfiles.d/union.conf
sudo systemctl daemon-reload
sudo systemctl enable --now union.service
```

systemd 单元启用了只读系统目录、私有临时目录、设备隔离、能力收缩和有限可写路径。若改变数据或博客目录，必须同步更新 `ReadWritePaths`。

## 7. 配置 Caddy

以 `config/Caddyfile.example` 为基线，替换三个域名和证书策略：

- 博客主机托管 `back/blog/dist`；
- 管理主机托管 `back/back/dist`，并把 `/api/*` 代理到 `127.0.0.1:8080`；
- 文件主机代理到 `127.0.0.1:5000`。

```bash
sudo caddy validate --config /etc/caddy/Caddyfile
sudo systemctl reload caddy
```

内网使用 `tls internal` 时，客户端必须信任 Caddy 根证书。公网域名应移除该行并使用 ACME。

## 8. 验收

```bash
systemctl status union --no-pager
journalctl -u union -n 100 --no-pager
curl --fail http://127.0.0.1:8080/api/health
curl --fail http://127.0.0.1:8080/api/ready
```

`ram` 只有在数据库中的期望状态为运行或从管理台启动后才监听 5000；启动后再验证：

```bash
curl --fail http://127.0.0.1:5000/__ram__/health
```

还应验证：

- 三个 HTTPS 域名均可访问；
- 管理员可登录并立即修改初始密码；
- ram 至少有一个强密码账号；
- 博客可以保存、发布并成功构建；
- 备份脚本能生成 age 加密归档和校验文件；
- 主机重启后服务能自动恢复。
