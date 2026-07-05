# Back 开发说明

## 环境

- Linux；
- Node.js 版本以仓库根目录 `.node-version` 为准；
- npm 主版本以 `back/source/package.json` 的 `engines` 为准；
- 本地联调需要 Union API 运行在 `127.0.0.1:8080`。

## 本地启动

```bash
cd back/source
npm ci
npm run dev
```

Vite 开发服务器监听 `127.0.0.1:3000`，并把 `/api/*` 代理到 Union。页面代码始终调用相对路径 `/api/*`，不要在组件里写死后端主机名。

## 关键源码

| 路径 | 说明 |
| --- | --- |
| `source/src/main.tsx` | React 挂载入口 |
| `source/src/App.tsx` | 顶层页面和认证状态组织 |
| `source/src/api.ts` | HTTP 请求封装、超时、错误解析、CSRF 头 |
| `source/src/types.ts` | 与 Union API 对齐的 TypeScript 类型 |
| `source/src/views/` | 各业务视图 |
| `source/src/styles/` | 分域样式文件 |

## 修改 API 时的同步点

修改 Union 的路由、JSON 字段或错误语义时，同步检查：

- `union/source/src/routes/`
- `union/source/src/domain/`
- `back/source/src/api.ts`
- `back/source/src/types.ts`
- `union/doc/api.md`

前端页面不直接拼复杂业务 URL，新增路径辅助函数放在 `source/src/api-paths.ts`。

## 构建与验证

```bash
cd back/source
npm run build
npm audit --audit-level=high
```

构建流程会先执行 TypeScript 项目构建，再由 Vite 输出 `dist.next/`，最后通过 `scripts/publish-static.mjs` 原子切换为 `dist/`。

## 常见问题

| 现象 | 检查 |
| --- | --- |
| 页面能打开但接口 404 | Vite 代理或 Caddy `/api/*` 反代是否指向 Union |
| 登录后马上失效 | 管理端和 API 是否同源，Cookie 是否被浏览器拦截 |
| 类型错误 | `types.ts` 是否与 Union `domain/` 中的响应结构一致 |
| 构建成功但页面旧资源未更新 | Caddy 是否托管当前 `back/source/dist` |
