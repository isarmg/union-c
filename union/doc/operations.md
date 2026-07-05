# 运行维护

## 日常检查

```bash
systemctl is-active postgresql union caddy
curl --fail http://127.0.0.1:8080/api/ready
df -h /opt/union /srv/backup
journalctl -u union --since today --no-pager
```

`/api/health` 表示 HTTP 进程可响应；`/api/ready` 还会做 PostgreSQL 最小往返，更适合作为流量就绪探针。

## 日志位置

| 日志 | 位置 |
| --- | --- |
| union | `journalctl -u union` |
| ram 进程 | `ram/data/logs/ram-process.log` |
| ram HTTP | `ram/data/logs/ram.log` |
| 博客构建 | `blog/data/logs/` |
| Caddy | `config/Caddyfile.example` 中配置的 `/var/log/caddy/*.json` |

不要把敏感环境、完整认证头或生成的 ram 私有配置写入工单和聊天记录。

## 更新流程

1. 生成并验证加密备份；
2. 在独立构建目录检出新版本；
3. 按 CI 命令执行 Rust 测试、Clippy、Web 构建和依赖审计；
4. 更新 `/opt/union` 的源码和锁文件；
5. 重新构建并原子替换 `union`、`ram` 二进制；
6. 更新两个 Web `dist/`；
7. 检查并安装变化的 systemd/Caddy/tmpfiles/logrotate 配置；
8. 重启 `union`，重载 Caddy；
9. 检查 ready、登录、博客构建和 ram；
10. 保留上一个已验证版本，直到观察窗口结束。

不要在运行中的部署目录里执行会长时间修改 `node_modules` 或 `target` 的操作。更稳妥的方式是在独立目录构建，再发布产物。

### 回滚条件

出现以下任一情况，应停止继续变更并回滚到上一个已验证版本：数据库初始化失败、`/api/ready` 持续失败、管理端无法登录、博客产物不完整，或 ram 无法恢复原期望状态。

代码与静态产物可以回滚，数据库结构和数据不能靠覆盖文件回滚。若新版本已修改数据库，应使用升级前的同一恢复点恢复数据库、运行文件和主密钥，并按 [PostgreSQL 恢复流程](database.md#恢复) 验收。

## 博客维护

文章管理源是 PostgreSQL。保存文章时先提交数据库，再生成完整临时内容目录并原子替换
`blog/data/content/`；发布或首页配置变化会触发构建调度。

构建使用合并调度和全局信号量，避免多个请求同时覆盖 `dist`。发布脚本先生成 `dist.next`，成功后切换到 `dist`。若构建失败，查看 `blog/data/logs/`，不要手工把不完整的 `dist.next` 当成线上目录。

不要直接把手写 Markdown 当作唯一内容源。必须人工导入时，把文件放入内容目录后调用
`POST /api/blog/import-orphans`；系统只会以草稿导入，正常启动和构建不会自动反向写数据库。

## ram 维护

- 账号和路径权限在管理台保存到 PostgreSQL；
- 保存后，运行中的 ram 会重载；
- 生成配置为 `ram/data/ram.generated.yaml`，权限必须是 `0600`；
- 若端口已被未托管进程占用，`union` 会拒绝杀死该进程；
- 生产环境拒绝空权限和弱密码；
- ram 内容必须和数据库权限配置处于同一备份周期。

## 备份计划

建议至少：

- 每日一次加密完整备份；
- 更新前额外备份；
- 备份复制到另一台主机或离线介质；
- 定期执行实际恢复演练；
- 监控归档年龄、大小和 `sha256sum` 校验。

仅看到备份文件不等于可恢复。恢复演练必须验证主密钥、数据库、博客资源和 ram 文件同时可用。

## 故障处理

排障时先记录失败时间、部署版本、请求 ID 和最近一次变更，再查看对应日志。不要先清空数据目录、重置数据库或反复覆盖环境文件，这些操作会破坏现场。

### union 无法启动

依次检查环境文件权限、数据库连通性、主密钥长度、生产 HTTPS 配置、两个 Web `dist/index.html` 和 systemd 可写路径。

### ready 失败

检查 PostgreSQL 服务、连接串、角色密码、数据库所有权、连接数和磁盘。不要用重启循环掩盖持续的认证失败。

### 博客构建失败

查看最新构建日志，确认 `blog/data/content/` 中 frontmatter 符合 schema，Node 版本正确且 `node_modules` 完整。修复后从管理台重新构建。

### ram 无法启动

检查 `ram` 是否在 systemd 的 `PATH` 中、端口 5000 是否占用、生成配置权限、数据目录权限和账号规则。

### 密钥丢失

从同一恢复点取回环境文件或 `union/data/union.secret`。没有原主密钥时，不应尝试猜测或自动降级为明文；需要重置所有外部凭据。
