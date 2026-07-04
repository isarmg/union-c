import { existsSync, renameSync, rmSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const appRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const next = resolve(appRoot, "dist.next");
const current = resolve(appRoot, "dist");
const previous = resolve(appRoot, "dist.previous");

if (!existsSync(next)) throw new Error("dist.next does not exist");
rmSync(previous, { recursive: true, force: true });
if (existsSync(current)) renameSync(current, previous);
try {
  renameSync(next, current);
  rmSync(previous, { recursive: true, force: true });
} catch (error) {
  if (!existsSync(current) && existsSync(previous)) renameSync(previous, current);
  throw error;
}
