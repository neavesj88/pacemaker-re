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

test("no entry is only a link: every setting can be changed in this view", () => {
  for (const f of areaFiles()) {
    assert.doesNotMatch(readFileSync(f.file, "utf8"), /how:\s*"view"/, `${f.dir}/settings.ts: put the option fields in a component and show it inline`);
  }
});

// A setting kept in the browser storage of the app window (`loadPref` / `savePref`) is a feature
// setting too, unless it only remembers how a view looks. Each key is either an entry or one of
// the view-state keys below, with the reason; a new key must be added to one of the two.
const PREF_ENTRIES = {
  "pacemaker.keyDetect": "key-detection",
  "pm.gridSuspects.v1": "suspect-grid",
  "pacemaker-revival.mixexport": "mix-export",
  "pacemaker-revival.tracklist": "tracklist",
  "pm.mixes.recipes.v1": "auto-transitions",
  "pm.setbuilder.prefs.v1": "set-builder",
  "pacemaker-revival.setlist.v1": "setlist-print",
  "pacemaker-revival.vdj.v1": "virtualdj-export",
  "pacemaker-revival.matcher.v1": "request-matcher",
  "pm.insights.gap.v1": "gap-map",
  "pacemaker.grid.vtempo.v1": "tempo-tools",
};
const VIEW_STATE = {
  "pm.tracks.view.v1": "which columns the track table shows and how it is sorted",
  "pm.cases.view.v1": "the case table's columns and sort, and the last choice in the merge dialog",
  "pm.mixes.list.v1": "how the mix list is sorted and filtered",
  "pm.mixes.tools.v1": "whether transitions are shown in the mix editor",
  "pm.mixes.sectionMarkers.v1": "whether section markers are drawn on clips",
  "pm.gridBatchNudgeMs": "the last nudge typed into the grid batch dialog",
};

function sourceFiles(dir) {
  return readdirSync(dir, { withFileTypes: true }).flatMap((d) => (d.isDirectory() ? sourceFiles(join(dir, d.name)) : /\.(ts|svelte)$/.test(d.name) && !d.name.includes("test") ? [join(dir, d.name)] : []));
}

test("every setting kept in browser storage has an entry, or is listed as view state", () => {
  const keys = new Set();
  for (const f of sourceFiles(join(appRoot, "src", "lib"))) {
    if (f.includes("devmock")) continue;
    for (const m of readFileSync(f, "utf8").matchAll(/(?:STORAGE|_KEY|KEY)\w*\s*=\s*"((?:pm|pacemaker)[a-zA-Z0-9.-]*)"/g)) keys.add(m[1]);
  }
  assert.ok(keys.size >= 15, `found ${[...keys]}`);
  const ids = new Set(all.map((x) => x.id));
  for (const k of keys) {
    assert.ok(PREF_ENTRIES[k] || VIEW_STATE[k], `${k} is stored in the browser: give it a Settings entry (PREF_ENTRIES) or say why it is view state (VIEW_STATE)`);
    if (PREF_ENTRIES[k]) assert.ok(ids.has(PREF_ENTRIES[k]), `${k} points at the entry "${PREF_ENTRIES[k]}", which does not exist`);
  }
  for (const k of [...Object.keys(PREF_ENTRIES), ...Object.keys(VIEW_STATE)]) assert.ok(keys.has(k), `${k} is listed here but no code stores it`);
});

// A settings file the backend keeps in the app's config folder is a feature setting. Each must
// be reachable from the Settings view; add the entry (and its row here) when a new one appears.
const FILES = {
  "bpm-range-settings.json": "bpm-range",
  "duplicate-settings.json": "duplicates",
  "energy-settings.json": "energy-auto",
  "energy-source.json": "energy-source",
  "export-prefs.json": "case-export",
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

// The Settings view has no feature page behind it, so a shown component that edits a setting
// held in a shared state object must save it itself (this once left "Write tags to files"
// unsaved). The state objects that save from their own page's effect are listed with the call
// that saves them; a component that touches one must make that call.
const SAVED_FROM_PAGE = [{ state: "cleanupState.svelte", save: "scheduleSave" }];

test("an inline entry whose component edits page-saved state saves it itself", () => {
  let checked = 0;
  for (const f of areaFiles()) {
    const src = readFileSync(f.file, "utf8");
    const imports = new Map([...src.matchAll(/import\s+(\w+)\s+from\s+"(\.[^"]+\.svelte)"/g)].map((m) => [m[1], resolve(dirname(f.file), m[2])]));
    for (const m of src.matchAll(/component:\s*(\w+)/g)) {
      const file = imports.get(m[1]);
      if (!file) continue;
      const code = readFileSync(file, "utf8");
      for (const { state, save } of SAVED_FROM_PAGE) {
        if (!code.includes(state)) continue;
        checked++;
        assert.ok(code.includes(save), `${f.dir}/settings.ts: ${m[1]} uses ${state} but never calls ${save}(), so a change made in the Settings view is not saved`);
      }
    }
  }
  assert.ok(checked >= 2, `checked ${checked}`);
});
