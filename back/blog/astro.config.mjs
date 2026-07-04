import { defineConfig } from "astro/config";
import mdx from "@astrojs/mdx";
import sitemap from "@astrojs/sitemap";
import { createReadStream, mkdirSync, statSync } from "node:fs";
import { extname, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";

const site = process.env.PUBLIC_SITE_URL ?? "https://home.lan";
const base = normalizeBasePath(process.env.ASTRO_BASE_PATH ?? "/");
const workspaceRoot = fileURLToPath(new URL("../..", import.meta.url));
const blogAssetRoot = resolve(workspaceRoot, "data", "blog", "files");
const blogContentRoot = resolve(workspaceRoot, "data", "blog", "content");

// blog 可独立构建；空仓库中没有运行时 data/ 时先创建只读输入目录。
mkdirSync(blogAssetRoot, { recursive: true });
mkdirSync(blogContentRoot, { recursive: true });

export default defineConfig({
  site,
  base,
  devToolbar: { enabled: false },
  integrations: [mdx(), sitemap()],
  vite: {
    plugins: [blogAssetPlugin(), watchBlogDataPlugin()],
    server: {
      fs: {
        // 允许 Vite 开发服务器读取工作区根目录下的所有文件
        allow: [workspaceRoot]
      }
    }
  },
  markdown: {
    shikiConfig: {
      theme: "github-dark"
    }
  }
});

function normalizeBasePath(value) {
  const trimmed = value.trim();

  if (!trimmed || trimmed === "/") {
    return "/";
  }

  return `/${trimmed.replace(/^\/+/, "").replace(/\/+$/, "")}`;
}

/**
 * 开发模式下把 data/blog/content 和 data/blog/files 加入 Vite 文件监听。
 * 这两个目录在 back/blog/ 外部，Vite 默认不监听，导致控制台写入新文章或
 * 修改现有文章后博客开发服务器不刷新内容。
 */
function watchBlogDataPlugin() {
  return {
    name: "watch-blog-data",
    configureServer(server) {
      server.watcher.add(blogContentRoot);
      server.watcher.add(blogAssetRoot);

      // 当 .site.json 或 .taxonomy.json 改变时触发整页刷新，
      // 因为这些文件通过 fs.readFileSync 读取，不在 Vite 模块图中，
      // 需要手动通知浏览器重新加载以反映最新配置。
      server.watcher.on("change", (file) => {
        if (
          file === resolve(blogContentRoot, ".site.json") ||
          file === resolve(blogContentRoot, ".taxonomy.json")
        ) {
          server.ws.send({ type: "full-reload" });
        }
      });
    }
  };
}

/**
 * 开发模式下把 /blog-assets/* 请求映射到 data/blog/files/ 目录。
 * 生产构建时由 copy-blog-assets.mjs 脚本把同一目录复制到 dist/blog-assets/。
 */
function blogAssetPlugin() {
  return {
    name: "blog-asset-dev-server",
    configureServer(server) {
      server.middlewares.use("/blog-assets", (request, response, next) => {
        if (request.method !== "GET" && request.method !== "HEAD") {
          next();
          return;
        }

        const pathname = decodeURIComponent(
          new URL(request.url ?? "/", "http://local.test").pathname
        );
        const relativePath = pathname
          .replace(/^\/blog-assets\/?/, "")
          .replace(/^\/+/, "");
        const filePath = resolve(blogAssetRoot, relativePath);
        const rootWithSep = blogAssetRoot.endsWith(sep)
          ? blogAssetRoot
          : `${blogAssetRoot}${sep}`;

        if (filePath !== blogAssetRoot && !filePath.startsWith(rootWithSep)) {
          response.statusCode = 403;
          response.end("Forbidden");
          return;
        }

        try {
          const stats = statSync(filePath);
          if (!stats.isFile()) {
            next();
            return;
          }
          response.setHeader("Content-Type", contentTypeFor(filePath));
          response.setHeader("Content-Length", stats.size.toString());
          if (request.method === "HEAD") {
            response.end();
            return;
          }
          createReadStream(filePath).pipe(response);
        } catch {
          next();
        }
      });
    }
  };
}

function contentTypeFor(filePath) {
  switch (extname(filePath).toLowerCase()) {
    case ".avif":
      return "image/avif";
    case ".gif":
      return "image/gif";
    case ".jpg":
    case ".jpeg":
      return "image/jpeg";
    case ".png":
      return "image/png";
    case ".svg":
      return "image/svg+xml";
    case ".webp":
      return "image/webp";
    default:
      return "application/octet-stream";
  }
}
