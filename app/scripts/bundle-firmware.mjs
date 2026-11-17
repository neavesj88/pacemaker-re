// Copies the newest work/build/pacemaker-revival-<number>.pfw into src-tauri/firmware/, the folder the
// app ships as a resource (run by `tauri build` through beforeBuildCommand). Skips quietly when there
// is none: the app then simply has no firmware of its own to offer. Run from app/: npm run bundle-firmware
import { copyFileSync, existsSync, mkdirSync, readdirSync, rmSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..", "..");
const from = process.env.PM_FIRMWARE_BUILD_DIR ?? join(root, "work", "build");
const to = join(root, "app", "src-tauri", "firmware");

const numbered = existsSync(from)
  ? readdirSync(from)
      .map((f) => ({ f, m: /^pacemaker-revival-(\d+)\.pfw$/i.exec(f) }))
      .filter((x) => x.m)
      .map((x) => ({ f: x.f, n: Number(x.m[1]) }))
      .sort((a, b) => b.n - a.n)
  : [];

if (numbered.length === 0) {
  console.log(`bundle-firmware: no pacemaker-revival-*.pfw in ${from}; nothing bundled`);
} else {
  mkdirSync(to, { recursive: true });
  // only the newest ships: an older copy left from an earlier build goes
  for (const old of readdirSync(to)) if (/\.pfw$/i.test(old) && old !== numbered[0].f) rmSync(join(to, old));
  copyFileSync(join(from, numbered[0].f), join(to, numbered[0].f));
  console.log(`bundle-firmware: ${numbered[0].f}`);
}
