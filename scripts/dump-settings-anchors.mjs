// Dumps every System Settings pane id and its deep-link anchors (as reported by
// System Settings itself) to scripts/settings-anchors.json.
//   node scripts/dump-settings-anchors.mjs          → write the dump
//   node scripts/dump-settings-anchors.mjs --check  → also verify every catalog
//     entry's pane/anchor exists on this Mac (exit 1 if any are missing)
//   node scripts/dump-settings-anchors.mjs --pages  → print the catalog's pages
//     that have a control, as input for settings-controls-probe.swift
//   node scripts/dump-settings-anchors.mjs --check-howtos → check how-to app bundle
//     ids exist and settingsTopic links resolve
//   node scripts/dump-settings-anchors.mjs --verify-controls <probe.jsonl>
//     → check each topic's controlId/controlLabel was found on its page
// Re-run after macOS updates: anchor names change between releases.
import { execFileSync } from "node:child_process";
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const dumpPath = join(root, "scripts/settings-anchors.json");
const catalogPath = join(root, "src/lib/settingsCatalog.json");
const catalog = JSON.parse(readFileSync(catalogPath, "utf8"));
const pageKey = (t) => `${t.pane} ${t.anchor ?? "-"}`;

if (process.argv.includes("--pages")) {
  const pages = new Set(catalog.filter((t) => t.controlId || t.controlLabel).map(pageKey));
  console.log([...pages].join("\n"));
  process.exit(0);
}

if (process.argv.includes("--check-howtos")) {
  // Every how-to app must exist on this Mac (third-party apps like Zoom are
  // reported, not failed) and every settingsTopic cross-link must exist.
  const howtos = JSON.parse(readFileSync(join(root, "src/lib/howtoCatalog.json"), "utf8"));
  const settingIds = new Set(catalog.map((t) => t.id));
  const problems = [];
  const ids = howtos.map((t) => t.id);
  for (const id of ids.filter((id, i) => ids.indexOf(id) !== i)) problems.push(`duplicate how-to id: ${id}`);
  for (const t of howtos) {
    if (t.settingsTopic && !settingIds.has(t.settingsTopic)) problems.push(`${t.id}: settingsTopic ${t.settingsTopic} not in settings catalog`);
    if (!t.app) continue;
    const found = execFileSync("mdfind", [`kMDItemCFBundleIdentifier == '${t.app}'`], { encoding: "utf8" }).trim();
    if (!found) {
      if (t.app.startsWith("com.apple.")) problems.push(`${t.id}: app ${t.app} not found`);
      else console.log(`note: ${t.id}: third-party app ${t.app} not installed here`);
    }
  }
  if (problems.length) {
    console.error(`${problems.length} how-to problem(s):\n  ${problems.join("\n  ")}`);
    process.exit(1);
  }
  console.log(`how-tos OK: ${howtos.length} topics`);
  process.exit(0);
}

const verifyIdx = process.argv.indexOf("--verify-controls");
if (verifyIdx >= 0) {
  // Mirrors NormalizeLabel + prefix match in src-tauri/src/settings_nav.m.
  const norm = (s) =>
    s.toLowerCase().replace(/[\u2010-\u2013]/g, "-").replace(/\u2019/g, "'").replace(/[\u00a0\u202f]/g, " ");
  const probed = new Map(
    readFileSync(process.argv[verifyIdx + 1], "utf8")
      .split("\n")
      .filter(Boolean)
      .map((l) => JSON.parse(l))
      .map((p) => [`${p.pane} ${p.anchor}`, p.controls])
  );
  let bad = 0;
  for (const t of catalog.filter((t) => t.controlId || t.controlLabel)) {
    const controls = probed.get(pageKey(t));
    const hit = controls?.find((c) =>
      t.controlId ? c.id === t.controlId : norm(c.label).startsWith(norm(t.controlLabel))
    );
    if (!hit) bad++;
    console.log(`${hit ? "✓" : "✗"} ${t.id.padEnd(26)} ${t.controlId ?? `label "${t.controlLabel}"`}${hit ? ` → ${hit.role}` : controls ? "" : "  (page not probed)"}`);
  }
  console.log(bad ? `${bad} control(s) not found` : "all controls found");
  process.exit(bad ? 1 : 0);
}

const jxa = `
const ss = Application("System Settings");
const out = {};
ss.panes().forEach((p) => {
  let a = [];
  try { a = p.anchors.name(); } catch (e) {}
  out[p.id()] = a;
});
JSON.stringify(out);
`;

const dump = JSON.parse(execFileSync("osascript", ["-l", "JavaScript", "-e", jxa], { encoding: "utf8" }));
const osVersion = execFileSync("sw_vers", ["-productVersion"], { encoding: "utf8" }).trim();
// System Settings sometimes answers with a near-empty list while it's still
// launching or quitting; never overwrite a good dump with that.
if (Object.keys(dump).length < 10) {
  console.error(`System Settings returned only ${Object.keys(dump).length} pane(s) — probably still starting up. Try again; ${dumpPath} left unchanged.`);
  process.exit(2);
}
writeFileSync(dumpPath, JSON.stringify({ macos: osVersion, panes: dump }, null, 2) + "\n");

const anchorCount = Object.values(dump).reduce((n, a) => n + a.length, 0);
console.log(`macOS ${osVersion}: ${Object.keys(dump).length} panes, ${anchorCount} anchors → ${dumpPath}`);

if (process.argv.includes("--check")) {
  const problems = [];
  for (const t of catalog) {
    if (!(t.pane in dump)) problems.push(`${t.id}: pane ${t.pane} not found`);
    else if (t.anchor && !dump[t.pane].includes(t.anchor)) problems.push(`${t.id}: anchor ${t.anchor} not in ${t.pane}`);
  }
  const ids = catalog.map((t) => t.id);
  for (const id of ids.filter((id, i) => ids.indexOf(id) !== i)) problems.push(`duplicate id: ${id}`);

  if (problems.length) {
    console.error(`${problems.length} catalog problem(s):\n  ${problems.join("\n  ")}`);
    process.exit(1);
  }
  console.log(`catalog OK: ${catalog.length} topics all resolve on macOS ${osVersion}`);
}
