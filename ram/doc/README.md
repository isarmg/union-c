# Ram 文件服务文档

`ram/` 是独立 Rust 文件服务项目，负责文件浏览、上传、下载、认证、WebDAV、压缩、哈希和 HTTP 日志。

## 目录职责

| 子目录 | 内容 |
| --- | --- |
| `source/` | Rust 源码、Cargo manifest、内嵌前端资源和许可证 |
| `data/` | 文件根目录、访问日志、进程日志和生成配置 |
| `doc/` | 开发、部署和运行边界说明 |

## 文档

| 目标 | 文档 |
| --- | --- |
| 本地开发、测试、构建 | [development.md](development.md) |
| 独立 Linux 部署 | [deployment.md](deployment.md) |
| 四机拓扑 | [../../union/doc/four-linux-deployment.md](../../union/doc/four-linux-deployment.md) |

## 运行模型

- ram 可以作为普通独立进程运行。
- ram 可以由 Union 在同机托管启动。
- ram 也可以部署在独立 Linux 主机，并在 Union 中登记为远程 RAM 实例。
- 远程管理接口是 `__ram__/admin/auth`，Union 用它同步账号规则。

Union 不复制文件服务能力，只管理配置、账号、期望状态和探测。
