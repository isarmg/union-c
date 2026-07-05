# 配置说明

## 配置分层

配置分为两层：

1. 启动环境：数据库连接、加密主密钥、生产模式等必须在进程启动前提供的值；
2. PostgreSQL 运行配置：监听端口、数据目录、外部主机、ram 和博客构建设置；
3. 前端构建环境：`PUBLIC_SITE_URL` 只在 Astro 构建时读取，不是 `union` 的运行参数。

基础运行配置以加密 JSON 存入 `settings.app.runtime_settings`。Sunshine 和 Proxmox
主机存入 `external_hosts`：地址和脱敏配置结构化保存，密码或 Token 单独加密。
数据库连接串保存在权限为 `0600` 的本地管理员配置中，不写入业务数据库。

初学者可以把加载过程理解为两个阶段：

```text
进程环境 / 本地私有配置
  -> 得到数据库地址和解密密钥
  -> 连接 PostgreSQL
  -> 读取 settings 表中的运行配置
  -> 应用允许的环境覆盖项
  -> 得到最终 Settings
```

之所以不能一次读完，是因为数据库里的配置只有先连接数据库后才能获得。`UNION_DATABASE_URL` 和 `UNION_SECRET_KEY` 属于“打开配置仓库所需的钥匙”，监听端口、博客路径和外部主机等属于仓库里的运行配置。排查配置未生效时，应先确认它属于哪一层，再检查进程实际继承的环境，而不是只查看当前终端。

## 环境变量

生产模板位于 `.env.production.example`。

| 变量 | 必需 | 说明 |
| --- | --- | --- |
| `UNION_ENV` | 生产必需 | 值为 `production` 时启用生产安全校验。 |
| `UNION_DATABASE_URL` | 可选 | PostgreSQL URL；也可登录控制台后在“设置”中保存。环境变量存在时优先。 |
| `UNION_SECRET_KEY` | 生产必需 | 32 字节随机值的 Base64 编码，用于 AES-256-GCM。 |
| `UNION_SECRET_KEY_ID` | 建议 | 当前密钥标识，默认 `primary`；写入 v2 密文用于识别密钥版本。 |
| `UNION_BOOTSTRAP_PASSWORD` | 首次生产启动必需 | 本地管理员配置不存在时使用，至少 12 字符；创建后应从环境文件删除。 |
| `UNION_RAM_PUBLIC_URL` | 生产必需 | ram 对外 HTTPS 地址，例如 `https://files.home.lan`。 |
| `UNION_RETENTION_DAYS` | 可选 | 会话外的运维历史保留天数，默认 90，范围 7 到 3650。 |
| `PUBLIC_SITE_URL` | 博客构建建议 | Astro 生成 canonical URL 和 sitemap 的站点地址；修改后需重建博客。 |
| `UNION_REQUIRE_LOCAL_STATIC_ARTIFACTS` | 可选 | 设为 `1` 时，生产启动会要求 Union 主机本地存在 `back/source/dist` 和 `blog/source/dist`；四机部署保持 `0`。 |
| `RUST_LOG` | 可选 | Rust 日志过滤规则。 |

生成主密钥：

```bash
openssl rand -base64 32
```

环境文件建议由 `root` 所有、服务组可读并设为 `0640`；若由服务账号所有，则收紧为 `0600`。不得让其他用户读取。

## 默认监听

| 组件 | 地址 | 生产要求 |
| --- | --- | --- |
| union | `127.0.0.1:8080` | 必须为回环地址，由 Caddy 反向代理。 |
| ram | `127.0.0.1:5000` | 必须为回环地址，由 Caddy 反向代理。 |
| back dev | `127.0.0.1:3000` | 只用于开发。 |
| blog dev | `127.0.0.1:4321` | 只用于开发。 |

## 运行时目录

默认目录全部相对于仓库/部署根目录：

| 路径 | 用途 |
| --- | --- |
| `union/data/union.secret` | 仅开发模式自动生成的本地主密钥。 |
| `blog/data/content/` | PostgreSQL 导出的文章、站点配置和分类索引。 |
| `blog/data/files/` | 博客图片和附件源文件。 |
| `blog/data/logs/` | Astro 构建日志。 |
| `ram/data/files/` | ram 服务根目录。 |
| `ram/data/files/public/` | 公开文件。 |
| `ram/data/files/inbox/` | 上传暂存。 |
| `ram/data/files/private/` | 私有文件。 |
| `ram/data/files/media/` | 媒体文件。 |
| `ram/data/logs/` | ram 访问日志和进程日志。 |
| `ram/data/ram.generated.yaml` | union 生成的私有 ram 配置。 |
| `union/data/sunshine/` | Sunshine 相关运行数据与日志。 |
| `union/data/moonlight/` | Moonlight 相关运行数据。 |

`union` 启动时会创建所需目录并把各项目 `data/` 权限设为 `0700`。

管理台保存数据库连接时会先连接并执行迁移，但不会热切换运行中的配置和后台任务；
页面提示重启后，由启动流程一次性装载完整数据库状态。

## ram 默认设置

- 命令：`ram`（内部服务协议仍为 ram）；
- 路径前缀：`/files`；
- 认证方式：Digest；
- 默认关闭上传、删除、符号链接和 CORS；
- 默认开启搜索、压缩下载和哈希；
- 生产环境必须至少配置一个非弱密码账号；
- 账号及权限保存在 PostgreSQL，密码加密存储；
- 实际启动参数只引用权限为 `0600` 的生成配置，避免密码出现在进程列表。

## blog 默认设置

- 工作目录：`blog/source`；
- 构建命令：`npm run build`；
- 内容目录：`blog/data/content`；
- 资源目录：`blog/data/files`；
- 构建采用 `dist.next -> dist` 原子切换，失败时尽量保留上一个可用版本。

## 生产校验

设置 `UNION_ENV=production` 后，启动会拒绝：

- 非回环的 union 或 ram 监听地址；
- 非 HTTPS 的 ram 公网地址；
- 已知默认或弱 ram 密码；
- `UNION_REQUIRE_LOCAL_STATIC_ARTIFACTS=1` 时缺失的 back/blog 静态构建产物；
- 缺失的加密主密钥；
- 首次建库时缺失或过短的管理员初始密码。
