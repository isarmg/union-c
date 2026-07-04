// ─────────────────────────────────────────────────────────────────────────────
// URL 路径工具函数
//
// 为什么需要 withBase / withoutBase？
// ─────────────────────────────────────────────────────────────────────────────
// 博客可能部署在两种场景：
//   A. 域名根路径：https://example.com/        → basePath = "/"
//   B. 子路径下：  https://example.com/blog/   → basePath = "/blog/"
//
// 场景 B 中，所有内部链接都需要带上前缀 "/blog/"，否则浏览器会找错位置。
// 例如首页链接应该是 "/blog/"，文章链接是 "/blog/posts/my-article/"。
//
// `withBase(path)`   → 给路径加上部署前缀（如果还没有的话）
// `withoutBase(path)` → 从路径中去掉部署前缀（用于比较路径是否"匹配"）
//
// `import.meta.env.BASE_URL` 是 Vite/Astro 构建时注入的环境变量，
// 对应 astro.config.mjs 中的 `base` 配置项。
// ─────────────────────────────────────────────────────────────────────────────

// `import.meta.env.BASE_URL`：Astro/Vite 在构建时根据配置自动注入，
// 部署在根路径时为 "/"，部署在 /blog/ 时为 "/blog/"。
const rawBasePath = import.meta.env.BASE_URL || "/";

// 规范化后的 basePath 始终以 "/" 开头并以 "/" 结尾（根路径时为 "/"）。
export const basePath = normalizeBasePath(rawBasePath);

// 这些路径需要保持绝对路径，不添加部署前缀。
// 原因：/files 由 ram 文件服务直接处理，与博客的部署路径无关。
const rootAbsolutePaths = ["/files"];

/**
 * 给路径添加部署前缀（base path）。
 *
 * 特殊情况处理：
 * - 空路径 → 返回 basePath 本身；
 * - 以协议（http:）、双斜杠（//）或锚点（#）开头的路径 → 原样返回（外部链接）；
 * - rootAbsolutePaths 中的路径 → 不添加前缀（绝对路径例外）；
 * - 路径已经包含 basePath 前缀 → 不重复添加（幂等性）。
 */
export function withBase(path: string): string {
  if (!path) {
    return basePath;
  }

  // 正则：匹配以协议（如 https:）、双斜杠（//）或 # 开头的路径 → 外部链接，原样返回
  if (/^(?:[a-z][a-z\d+.-]*:|\/\/|#)/i.test(path)) {
    return path;
  }

  const pathWithSlash = path.startsWith("/") ? path : `/${path}`;
  const baseWithoutSlash = basePath.slice(0, -1); // 去掉末尾的 /，用于精确匹配

  // rootAbsolutePaths 中的路径（如 /files）不添加部署前缀
  if (
    rootAbsolutePaths.some(
      (prefix) => pathWithSlash === prefix || pathWithSlash.startsWith(`${prefix}/`)
    )
  ) {
    return pathWithSlash;
  }

  // 已经带前缀，或部署在根路径，直接返回
  if (
    basePath === "/" ||
    pathWithSlash === baseWithoutSlash ||
    pathWithSlash.startsWith(basePath)
  ) {
    return pathWithSlash;
  }

  // 拼接前缀，注意去掉 pathWithSlash 开头多余的 /，避免产生双斜杠
  return `${basePath}${pathWithSlash.replace(/^\/+/, "")}`;
}

/**
 * 从路径中去掉部署前缀，返回"纯净"的相对路径。
 *
 * 主要用途：比较当前页面路径与导航链接是否匹配（active 高亮），
 * 不受部署前缀影响。
 *
 * 例如：basePath = "/blog/"
 *   withoutBase("/blog/posts/hello") → "/posts/hello"
 *   withoutBase("/blog/") → "/"
 *   withoutBase("/other") → "/other"（不含前缀，原样返回）
 */
export function withoutBase(pathname: string): string {
  const pathWithSlash = pathname.startsWith("/") ? pathname : `/${pathname}`;

  if (basePath === "/") {
    return pathWithSlash; // 根路径部署时无需处理
  }

  const baseWithoutSlash = basePath.slice(0, -1);

  // 精确匹配前缀路径（如 /blog），等价于根路径 /
  if (pathWithSlash === baseWithoutSlash) {
    return "/";
  }

  // 路径以 basePath 开头，去掉前缀
  if (pathWithSlash.startsWith(basePath)) {
    return `/${pathWithSlash.slice(basePath.length)}`;
  }

  // 路径不在 basePath 下，原样返回
  return pathWithSlash;
}

/**
 * 将路径转换为 CSS url() 格式，用于 CSS 自定义属性中的背景图设置。
 *
 * 例如：cssUrl("/images/bg.jpg") → `url("/blog/images/bg.jpg")`
 * 需要转义路径中的双引号和反斜杠，避免破坏 CSS 语法。
 */
export function cssUrl(path: string): string {
  return `url("${withBase(path).replace(/["\\]/g, "\\$&")}")`;
}

/**
 * 规范化 basePath：确保始终以 "/" 开头，以 "/" 结尾（根路径时为 "/"）。
 * 如果传入的是完整 URL，则提取其 pathname 部分。
 */
function normalizeBasePath(value: string): string {
  const trimmed = value.trim();

  if (!trimmed || trimmed === ".") {
    return "/";
  }

  let pathname = trimmed;

  // 如果是完整 URL（如 "https://example.com/blog/"），提取 pathname 部分
  if (/^[a-z][a-z\d+.-]*:/i.test(trimmed)) {
    pathname = new URL(trimmed).pathname;
  }

  // 规范化：去掉首尾多余的 /，再统一加上
  pathname = `/${pathname.replace(/^\/+/, "").replace(/\/+$/, "")}`;
  // 根路径返回 "/"，子路径返回带末尾斜杠的格式（如 "/blog/"）
  return pathname === "/" ? "/" : `${pathname}/`;
}
