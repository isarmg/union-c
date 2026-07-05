import { cpSync, existsSync, lstatSync, mkdirSync, rmSync } from "node:fs";
import { dirname, extname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const scriptRoot = dirname(fileURLToPath(import.meta.url));
const appRoot = resolve(scriptRoot, "..");
const source = resolve(appRoot, "..", "data", "files");
const outputDir = process.argv[2] ?? "dist.next";
const target = resolve(appRoot, outputDir, "blog-assets");
const publicExtensions = new Set([
  ".avif", ".gif", ".jpg", ".jpeg", ".png", ".svg", ".webp",
  ".pdf", ".txt", ".zip"
]);

if (!existsSync(source)) {
  mkdirSync(target, { recursive: true });
  process.exit(0);
}

rmSync(target, { recursive: true, force: true });
mkdirSync(target, { recursive: true });
cpSync(source, target, {
  recursive: true,
  filter(path) {
    const stats = lstatSync(path);
    if (stats.isSymbolicLink()) return false;
    if (stats.isDirectory()) return true;
    return !path.split("/").some((part) => part.startsWith("."))
      && publicExtensions.has(extname(path).toLowerCase());
  }
});
