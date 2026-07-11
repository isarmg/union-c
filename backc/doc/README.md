# BackC

BackC 是 UnionC 对应的 React 管理前端，提供总览、只读主机监控、Sunshine、Sunshine
日志和设置页面。主机页显示 CPU、内存、GPU、逐接口网络、逐挂载磁盘、温度、采集能力
和历史趋势；缺失能力显示 `N/A`，没有任何远程控制按钮。

## 开发运行

先启动 UnionC，然后运行：

```bash
cd backc/source
npm ci
npm run dev
```

开发服务器默认监听 `127.0.0.1:3001`，并把 `/api` 代理到 `127.0.0.1:8081`。

## 构建

```bash
npm run build
```

构建产物位于 `backc/source/dist/`。
