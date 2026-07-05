import { chmodSync, existsSync, lstatSync, readdirSync, renameSync, rmSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const appRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const next = resolve(appRoot, "dist.next");
const current = resolve(appRoot, "dist");
const previous = resolve(appRoot, "dist.previous");

function makeStaticTreeReadable(path) {
  const stat = lstatSync(path);
  if (stat.isSymbolicLink()) return;
  if (stat.isDirectory()) {
    chmodSync(path, 0o755);
    for (const entry of readdirSync(path)) {
      makeStaticTreeReadable(resolve(path, entry));
    }
    return;
  }
  if (stat.isFile()) chmodSync(path, 0o644);
}

if (!existsSync(next)) throw new Error("dist.next does not exist");
rmSync(previous, { recursive: true, force: true });
if (existsSync(current)) renameSync(current, previous);
try {
  renameSync(next, current);
  makeStaticTreeReadable(current);
  rmSync(previous, { recursive: true, force: true });
} catch (error) {
  if (!existsSync(current) && existsSync(previous)) renameSync(previous, current);
  throw error;
}
