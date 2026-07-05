# Back 独立部署

Back 可以部署在独立 Linux 主机上，只托管静态文件，并把 `/api/*` 转发到 Union 主机。

## 构建

```bash
cd back/source
npm ci
npm run build
```

生产只需要：

- `back/source/dist/`
- Caddy 或其他静态文件服务器；
- 到 Union 主机的 HTTPS 或可信内网连接。

## Caddy 示例

```caddyfile
admin.example.com {
    root * /opt/union/back/source/dist

    handle /api/* {
        reverse_proxy https://union-api.internal {
            transport http {
                dial_timeout 3s
                response_header_timeout 30s
            }
        }
    }

    handle {
        try_files {path} /index.html
        file_server
    }
}
```

## 安全边界

- 管理端不要直接暴露开发服务器。
- 浏览器访问管理端和 `/api/*` 应保持同源，避免 Cookie SameSite 限制。
- Union API 不应直接暴露公网；让 Back 主机或内网反代访问。
- 静态资源可长缓存，`index.html` 建议 `no-cache`。

## 验收

```bash
curl --fail https://admin.example.com/
curl --fail https://admin.example.com/api/health
```

登录后检查总览页、设置页和一条只读 API 请求。若 `/api/ready` 失败，应先到 Union 主机检查数据库和环境文件。
