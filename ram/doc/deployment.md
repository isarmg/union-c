# Ram 独立部署

Ram 可以独立部署在文件服务 Linux 主机上。生产环境推荐由 Caddy 提供 HTTPS，再反向代理到本机 ram 进程。

## 构建

```bash
cargo build --manifest-path ram/source/Cargo.toml --release --locked
sudo install -o root -g root -m 0755 ram/source/target/release/ram /usr/local/bin/ram
```

## 数据目录

```bash
sudo install -d -o ram -g ram -m 0700 /opt/ram/data
sudo install -d -o ram -g ram -m 0750 /opt/ram/data/files
sudo install -d -o ram -g ram -m 0700 /opt/ram/data/logs
```

## 配置示例

`/etc/ram/ram.yaml`：

```yaml
serve-path: "/opt/ram/data/files"
bind: "127.0.0.1"
port: 5000
path-prefix: "/"
auth-method: "digest"
auth:
  - "admin:replace-with-long-password@/:rw"
auth-state-file: "/opt/ram/data/ram.auth.yaml"
allow-search: true
allow-archive: true
allow-hash: true
log-file: "/opt/ram/data/logs/ram.log"
```

启动：

```bash
ram --config /etc/ram/ram.yaml
```

## Caddy 示例

```caddyfile
files.example.com {
    reverse_proxy 127.0.0.1:5000
}
```

## 作为 Union 远程实例

在 Union 管理台登记：

- 主机名或 IP；
- 端口；
- 是否启用 TLS；
- 是否校验证书；
- 管理账号。

生产环境远程 RAM 必须使用 HTTPS 且校验证书。Union 会通过 `__ram__/admin/auth` 更新远程认证规则。

## 验收

```bash
curl --fail https://files.example.com/__ram__/health
```

再用浏览器确认目录页、登录、下载和上传权限符合预期。
