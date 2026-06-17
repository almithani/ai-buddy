// Bumps the app version across tauri.conf.json (the version Tauri bundles),
// package.json, and src-tauri/Cargo.toml so they stay in sync.
//
// Usage: node scripts/bump-version.mjs [patch|minor|major]   (default: patch)
//
// Run BEFORE `tauri build` (see the npm "release" scripts) — Tauri reads
// tauri.conf.json's version when the build starts, so bumping it inside a
// beforeBuildCommand hook would only affect the *next* build.

import { readFileSync, writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const part = process.argv[2] || "patch";
if (!["patch", "minor", "major"].includes(part)) {
  console.error(`Unknown bump "${part}" — use patch | minor | major`);
  process.exit(1);
}

const tauriConfPath = join(root, "src-tauri", "tauri.conf.json");
const pkgPath = join(root, "package.json");
const cargoPath = join(root, "src-tauri", "Cargo.toml");

const tauriConf = JSON.parse(readFileSync(tauriConfPath, "utf8"));
const m = /^(\d+)\.(\d+)\.(\d+)$/.exec(tauriConf.version ?? "");
if (!m) {
  console.error(`Can't parse current version "${tauriConf.version}" (expected X.Y.Z)`);
  process.exit(1);
}
let [maj, min, pat] = m.slice(1).map(Number);
if (part === "major") { maj += 1; min = 0; pat = 0; }
else if (part === "minor") { min += 1; pat = 0; }
else { pat += 1; }
const next = `${maj}.${min}.${pat}`;

// tauri.conf.json (canonical — drives the bundle version)
tauriConf.version = next;
writeFileSync(tauriConfPath, JSON.stringify(tauriConf, null, 2) + "\n");

// package.json
const pkg = JSON.parse(readFileSync(pkgPath, "utf8"));
pkg.version = next;
writeFileSync(pkgPath, JSON.stringify(pkg, null, 2) + "\n");

// Cargo.toml — only the [package] version line (deps use inline `version = `,
// which never starts a line, so the multiline ^anchor is safe).
let cargo = readFileSync(cargoPath, "utf8");
cargo = cargo.replace(/^version = "[^"]+"/m, `version = "${next}"`);
writeFileSync(cargoPath, cargo);

console.log(`Version bumped ${m[0]} → ${next}`);
