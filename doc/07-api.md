# HTTP API

## 通用约定

- 默认基址：`http://127.0.0.1:8080`；
- 生产环境经管理域名的 Caddy HTTPS 入口访问；
- 请求和响应主体使用 JSON，封面图片接口除外；
- 默认最大请求体为 10 MiB；
- 除登录和健康检查外，接口需要有效会话；
- 浏览器 Cookie 发起的 POST、PUT、DELETE 等变更请求必须带 `X-CSRF-Token: 1`；
- 自动化客户端可使用 `Authorization: Bearer <session-token>`；
- 错误响应统一包含稳定机器码 `code`、HTTP 分类 `error` 和展示文本 `message`；客户端逻辑只能判断 `code`，不能解析自然语言文本；
- 每个请求由服务端生成或传播 `X-Request-ID`，审计日志记录操作人和请求 ID。

## 一次请求经过哪些代码

以管理端读取文章为例：页面调用 `back/back/src/api.ts` 中的 API 函数，请求到达 `back/union/src/routes/mod.rs` 后，先依次经过请求 ID、日志、请求体大小和访问控制中间件，再进入 `routes/blog.rs` 的具体处理函数。处理函数调用博客业务模块和 `database/blog/`，最后把 Rust 类型序列化为 JSON。

```text
React 页面 -> api.ts -> Caddy/Vite 代理 -> 全局中间件
           -> routes handler -> 业务模块 -> database -> PostgreSQL
           <-                  JSON 响应                  <-
```

因此排查问题时可以按层定位：浏览器没有发出请求，检查组件和 `api.ts`；收到 `401/403`，检查认证与 CSRF；收到 `4xx` 参数错误，检查路由输入类型；收到 `5xx`，结合 `X-Request-ID` 检查 Union 日志和上游服务。

`GET` 通常只读取数据；`POST` 常用于创建或执行动作；`PUT` 表示更新已有资源；`DELETE` 表示删除。这里描述的是本项目约定，判断接口是否有副作用仍应以路由文档和实现为准。

## 认证和健康

| 方法 | 路径 | 说明 |
| --- | --- | --- |
| POST | `/api/auth/login` | 登录并创建会话。 |
| GET | `/api/auth/basic` | HTTP Basic 登录兼容入口。 |
| POST | `/api/auth/logout` | 注销当前会话。 |
| GET | `/api/auth/me` | 当前用户。 |
| POST | `/api/auth/change-password` | 修改密码并处理会话。 |
| GET / PUT | `/api/settings/database` | 读取或测试并保存 PostgreSQL 连接；保存后立即切换连接池。 |
| GET | `/api/health` | HTTP 存活检查。 |
| GET | `/api/ready` | 包含 PostgreSQL 往返的就绪检查。 |

## 服务和 ram

| 方法 | 路径 | 说明 |
| --- | --- | --- |
| GET | `/api/services` | 所有服务状态。 |
| POST | `/api/services/ram/start` | 启动受管 ram。 |
| POST | `/api/services/ram/stop` | 停止受管 ram。 |
| POST | `/api/services/ram/restart` | 重启受管 ram。 |
| GET | `/api/services/ram/config` | 脱敏后的有效配置。 |
| GET | `/api/services/ram/command` | 脱敏后的启动命令。 |
| GET/POST | `/api/services/ram/auth` | 读取或全量保存账号权限。 |
| GET | `/api/services/ram/health` | ram 健康探测。 |
| GET | `/api/services/ram/entry?path=/...` | 读取目录 JSON。 |
| GET | `/api/services/ram/logs?lines=N` | 读取末尾日志。 |

远程 RAM 主机接口：

| 方法 | 路径 | 说明 |
| --- | --- | --- |
| GET / POST | `/api/services/ram/instances` | 列表或创建远程主机。 |
| PUT / DELETE | `/api/services/ram/instances/{id}` | 更新或删除远程主机。 |
| GET / POST | `/api/services/ram/instances/{id}/auth` | 读取或保存远程主机管理员凭据。 |

远程 RAM 程序自身提供受现有具名管理员认证保护的 `GET/PUT /__ram__/admin/auth`，用于热更新认证规则。使用 `--auth-state-file <file>`（或 `RAM_AUTH_STATE_FILE`）可把更新以 `0600` 权限持久化，供进程重启后恢复。

## Sunshine

主机 CRUD：

| 方法 | 路径 |
| --- | --- |
| GET/POST | `/api/services/sunshine/hosts` |
| PUT/DELETE | `/api/services/sunshine/hosts/{id}` |
| GET | `/api/services/sunshine/hosts/{id}/status` |
| POST | `/api/services/sunshine/hosts/{id}/wake` |
| GET | `/api/services/sunshine/hosts/{id}/logs` |

代理能力：

| 能力 | 路径 |
| --- | --- |
| 应用列表/保存 | `GET/POST .../hosts/{id}/apps` |
| 关闭应用 | `POST .../hosts/{id}/apps/close` |
| 删除应用 | `DELETE .../hosts/{id}/apps/{index}` |
| 客户端 | `GET .../clients`，`POST .../clients/unpair`、`unpair-all`、`update` |
| 配置 | `GET/POST .../config`，`GET .../config/locale` |
| 日志 | `GET .../api-logs` |
| 配对 | `POST .../pin` |
| 密码 | `POST .../password` |
| 系统 | `POST .../restart`、`POST .../reset-display` |
| 封面 | `GET .../covers/{index}`、`POST .../covers/upload` |

## blog

| 方法 | 路径 | 说明 |
| --- | --- | --- |
| GET/DELETE | `/api/blog/posts` | 列表或按 query path 删除。 |
| GET | `/api/blog/posts/detail?path=...` | 文章详情。 |
| POST | `/api/blog/posts/save` | 新建或保存文章。 |
| GET/POST | `/api/blog/home` | 首页配置。 |
| GET | `/api/blog/taxonomy` | 分类和标签。 |
| POST | `/api/blog/build` | 手动构建。 |
| GET | `/api/blog/logs` | 构建日志。 |
| POST | `/api/blog/publish` | 发布。 |
| POST | `/api/blog/unpublish` | 转为草稿。 |
| POST | `/api/blog/tags/add` | 新增标签。 |
| POST | `/api/blog/tags/rename` | 重命名标签。 |
| POST | `/api/blog/tags/delete` | 删除标签。 |
| POST | `/api/blog/categories/add` | 新增分类。 |
| POST | `/api/blog/categories/rename` | 重命名分类。 |
| POST | `/api/blog/categories/delete` | 删除分类。 |

所有文章 path 都会经过 Linux 相对路径校验，拒绝反斜杠、绝对路径、`..` 和非 `.md`/`.mdx` 扩展名。

## Proxmox VE

主机和集群：

- `GET/POST /api/pve/hosts`；
- `PUT/DELETE /api/pve/hosts/{id}`；
- `GET /api/pve/hosts/{id}/resources|nodes|tasks`；
- `GET /api/pve/hosts/{id}/nodes/{node}/status|storage|tasks`；
- `GET /api/pve/hosts/{id}/nodes/{node}/storage/{storage}/content`。

QEMU VM 基址：

```text
/api/pve/hosts/{id}/nodes/{node}/vms/{vmid}
```

支持 `status`、`config`、删除、`start`、`stop`、`shutdown`、`reboot`、`suspend`、`resume`、`reset`、`migrate`，以及快照列表/创建/删除/回滚。

LXC 基址：

```text
/api/pve/hosts/{id}/nodes/{node}/containers/{vmid}
```

支持 `status`、`config`、删除、`start`、`stop`、`shutdown`、`reboot`，以及快照列表/创建/删除/回滚。

## 系统与事件

| 方法 | 路径 | 说明 |
| --- | --- | --- |
| GET | `/api/system/resources` | CPU、内存、磁盘等 Linux 资源。 |
| POST | `/api/events/ticket` | 签发 60 秒 SSE 短票据。 |
| GET | `/api/events?ticket=...` | 服务和博客构建事件流。 |

SSE 使用短票据，避免把长期会话 token 留在 URL 和代理访问日志中。
