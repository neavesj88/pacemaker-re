// Run: node --experimental-strip-types --test src/lib/views/settings/settingsLogic.test.mjs
// The grouping rules of the Settings view, and two guards that keep it complete: every area's
// settings.ts is well formed, and every settings file the backend keeps has an entry here.
import assert from "node:assert/strict";
import { existsSync, readdirSync, readFileSync, statSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { AREAS } from "./areas.ts";
import { group, matches, problems, words } from "./settingsLogic.ts";

const here = dirname(fileURLToPath(import.meta.url));
const views = resolve(here, "..");
const appRoot = resolve(here, "../../../..");
const repoRoot = resolve(appRoot, "..");

const e = (id, area, title, help = "help", keywords) => ({ id, area, title, help, keywords });

test("a search keeps entries that contain every word, in title, help, keywords or area", () => {
  const one = e("a", "Cases", "Duplicate finder", "How strict each way of finding copies is", "merge keeper");
  assert.deepEqual(words("  Merge   KEEPER "), ["merge", "keeper"]);
  assert.equal(matches(one, ""), true, "an empty search keeps everything");
  assert.equal(matches(one, "duplicate"), true);
  assert.equal(matches(one, "STRICT copies"), true, "words may come from different fields");
  assert.equal(matches(one, "keeper"), true, "keywords count");
  assert.equal(matches(one, "cases"), true, "so does the area");
  assert.equal(matches(one, "duplicate stems"), false, "every word is needed");
});

test("groups follow the area order, drop empty areas and sort by title", () => {
  const list = [e("z", "Stems", "Stem separation"), e("b", "Library", "Metadata"), e("a", "Library", "Energy source"), e("c", "Cases", "Smart cases")];
  const g = group(list);
  assert.deepEqual(g.map((x) => x.area), ["Library", "Cases", "Stems"]);
  assert.deepEqual(g[0].entries.map((x) => x.id), ["a", "b"]);
  assert.deepEqual(group(list, "smart").map((x) => x.area), ["Cases"]);
  assert.deepEqual(group(list, "nothing like this"), []);
});

test("an entry whose area is not listed is shown last under its own name, never hidden", () => {
  const g = group([e("a", "Library", "One"), e("x", "Elsewhere", "Two")]);
  assert.deepEqual(g.map((x) => x.area), ["Library", "Elsewhere"]);
});

test("problems names a repeated id, an unknown area, a bad id and a blank text", () => {
  assert.deepEqual(problems([e("a", "Library", "One"), e("b", "Cases", "Two")]), []);
  const bad = problems([e("a", "Library", "One"), e("a", "Cases", "Two"), e("Bad Id", "Mixes", "Three"), e("c", "Nowhere", "Four"), e("d", "Mixes", " ", "")]);
  assert.equal(bad.length, 4, bad.join("; "));
  assert.ok(bad.some((p) => p.includes("used twice")) && bad.some((p) => p.includes("not a usable id")) && bad.some((p) => p.includes("not one of the areas")) && bad.some((p) => p.includes("title and a help")));
});

// ---- the real entries, read from the files -------------------------------------------------

function areaFiles() {
  return readdirSync(views)
    .filter((d) => statSync(join(views, d)).isDirectory() && existsSync(join(views, d, "settings.ts")))
    .map((d) => ({ dir: d, file: join(views, d, "settings.ts") }));
}

/** The `id`, `area` and `title` of each entry object in a settings.ts, read with a pattern (the files import Svelte, so node cannot load them). */
function entriesOf(source) {
  const out = [];
  const re = /\bid:\s*"([^"]+)",\s*\n\s*area:\s*"([^"]+)",\s*\n\s*title:\s*"((?:[^"\\]|\\.)*)",\s*\n\s*help:\s*"((?:[^"\\]|\\.)*)"/g;
  for (let m; (m = re.exec(source)); ) out.push({ id: m[1], area: m[2], title: m[3], help: m[4] });
  return out;
}

const all = areaFiles().flatMap((f) => entriesOf(readFileSync(f.file, "utf8")).map((x) => ({ ...x, dir: f.dir })));

test("every settings.ts is well formed: the pattern finds each entry, ids are unique, areas are listed", () => {
  for (const f of areaFiles()) {
    const src = readFileSync(f.file, "utf8");
    const declared = (src.match(/\bhow:\s*"/g) ?? []).length;
    assert.equal(entriesOf(src).length, declared, `${f.dir}/settings.ts: every entry needs id, area, title and help in that order`);
    assert.match(src, /export default entries;/, `${f.dir}/settings.ts exports its list`);
  }
  assert.ok(all.length >= 15, `found ${all.length} entries`);
  assert.deepEqual(problems(all), []);
  assert.ok(all.every((x) => AREAS.includes(x.area)));
});

test("every component an entry imports exists", () => {
  for (const f of areaFiles()) {
    const src = readFileSync(f.file, "utf8");
    for (const m of src.matchAll(/from\s+"(\.[^"]+)"/g)) {
      const target = resolve(dirname(f.file), m[1]);
      const found = [target, `${target}.ts`, `${target}.svelte.ts`].some((p) => existsSync(p));
      assert.ok(found, `${f.dir}/settings.ts imports ${m[1]}, which is not there`);
    }
  }
});

// A settings file the backend keeps in the app's config folder is a feature setting. Each must
// be reachable from the Settings view; add the entry (and its row here) when a new one appears.
const FILES = {
  "duplicate-settings.json": "duplicates",
  "energy-settings.json": "energy-auto",
  "energy-source.json": "energy-source",
  "export-prefs.json": "interop",
  "metadata-cleanup.json": "metadata-cleanup",
  "playlog-settings.json": "play-log",
  "relink.json": "device-relink",
  "sections-settings.json": "sections",
  "smart-settings.json": "smart-cases",
  "space-planner.json": "free-space",
  "stems-settings.json": "stems",
  "trust-settings.json": "device-safety",
  "watch-settings.json": "watch-folders",
};

function rustFiles(dir) {
  return readdirSync(dir, { withFileTypes: true }).flatMap((d) => (d.isDirectory() ? rustFiles(join(dir, d.name)) : d.name.endsWith(".rs") ? [join(dir, d.name)] : []));
}

test("every settings file the backend keeps has a Settings entry", () => {
  const sources = [...rustFiles(join(appRoot, "src-tauri", "src")), join(repoRoot, "crates", "pm-formats", "src", "ops_dupes.rs")];
  const found = new Set();
  for (const f of sources) {
    for (const m of readFileSync(f, "utf8").matchAll(/"([a-z0-9-]+\.json)"/g)) {
      if (/(settings|prefs|planner|source|relink|cleanup)/.test(m[1])) found.add(m[1]);
    }
  }
  assert.ok(found.size >= 10, `found ${[...found]}`);
  const ids = new Set(all.map((x) => x.id));
  for (const file of found) {
    assert.ok(FILES[file], `${file} is a settings file with no row in FILES: give it a Settings entry`);
    assert.ok(ids.has(FILES[file]), `${file} points at the entry "${FILES[file]}", which does not exist`);
  }
  for (const file of Object.keys(FILES)) assert.ok(found.has(file), `${file} is listed here but the backend no longer keeps it`);
});
