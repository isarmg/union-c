export const DEFAULT_BLOG_IMAGE_PATH = "/blog-assets/images/home-lab-hero.png";

const BLOG_ASSET_PREFIX = "/blog-assets/";

export function normalizeBlogAssetPath(
  value: string | null | undefined,
  fallback = DEFAULT_BLOG_IMAGE_PATH
): string {
  const cleaned = value?.trim() || fallback;
  if (/^(?:[a-z][a-z\d+.-]*:|\/\/|#)/i.test(cleaned)) {
    return cleaned;
  }
  if (cleaned.startsWith(BLOG_ASSET_PREFIX)) {
    return cleaned;
  }
  if (cleaned.startsWith("/")) {
    return `${BLOG_ASSET_PREFIX}${cleaned.replace(/^\/+/, "")}`;
  }
  return `${BLOG_ASSET_PREFIX}${cleaned.replace(/^\/+/, "")}`;
}
