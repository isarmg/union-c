/*
 * backc 的 Vite 开发/构建配置。
 *
 * Vite 负责启动前端开发服务器、编译 React/TypeScript，并在生产构建时输出静态文件。
 */
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

export default defineConfig({
  // 启用 React 插件，让 Vite 能识别 JSX/TSX 和 React Fast Refresh。
  plugins: [react()],
  server: {
    // 只监听本机，避免开发服务器直接暴露到局域网。
    host: "127.0.0.1",
    port: 3001,
    fs: {
      allow: ["../.."]
    },
    proxy: {
      // 开发环境下，前端请求 /api 会转发到 Rust 后端。
      // 这样组件代码可以一直写相对路径，不用区分开发/生产 API 地址。
      "/api": {
        target: "http://127.0.0.1:8081",
        changeOrigin: true
      }
    }
  }
});
