// @ts-check
// Builds the app into `dist/` (or the directory given as argument): the
// compiled TypeScript under `assets/`, plus the static files.
import { execFileSync } from "node:child_process";
import { cpSync, mkdirSync, rmSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const web = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const out = resolve(process.argv[2] ?? join(web, "dist"));

rmSync(out, { recursive: true, force: true });
mkdirSync(out, { recursive: true });
execFileSync(
  join(web, "node_modules", ".bin", "tsc"),
  ["-p", join(web, "tsconfig.json"), "--outDir", join(out, "assets")],
  { stdio: "inherit" },
);
cpSync(join(web, "static"), out, { recursive: true });
