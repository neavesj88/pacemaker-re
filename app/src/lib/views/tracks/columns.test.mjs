// Run: node src/lib/views/tracks/columns.test.mjs
import assert from "node:assert/strict";
import { test } from "node:test";
import { cleanOrder, cleanSort, COLUMNS, defaultPrefs, mergePrefs, moveColumn, shownColumns, stepTarget } from "./columns.ts";

const KEYS = COLUMNS.map((c) => c.key);

test("every sortable column names a distinct sort field and the new columns are hidden by default", () => {
  const fields = COLUMNS.flatMap((c) => (c.sort ? [c.sort] : []));
  assert.equal(new Set(fields).size, fields.length);
  assert.equal(new Set(KEYS).size, KEYS.length);
  for (const k of ["year", "label", "remixer", "composer", "comments", "added", "lastplayed", "format", "bitrate", "size"]) {
    const c = COLUMNS.find((x) => x.key === k);
    assert.ok(c, k);
    assert.equal(c.defaultOn, false, k);
    assert.ok(c.sort, k + " sorts");
  }
  assert.equal(COLUMNS[0].key, "fav");
});

test("defaults: every column once in the default order, sorted by artist, both settings on", () => {
  const d = defaultPrefs();
  assert.deepEqual(d.order, KEYS);
  assert.deepEqual(d.sort, [{ field: "artist", asc: true }]);
  assert.equal(d.rememberView, true);
  assert.equal(d.findClearsFilters, true);
});

test("saved prefs of the earlier shape (sortKey + sortAsc, no order) still load", () => {
  const p = mergePrefs({ visible: { album: true, bpm: false }, widths: { title: 300 }, sortKey: "bpm", sortAsc: false });
  assert.deepEqual(p.sort, [{ field: "bpm", asc: false }]);
  assert.equal(p.visible.album, true);
  assert.equal(p.visible.bpm, false);
  assert.equal(p.widths.title, 300);
  assert.deepEqual(p.order, KEYS);
});

test("junk in saved prefs is ignored", () => {
  for (const junk of [null, undefined, 5, "x", [], { sort: "x", order: 7, visible: 3, widths: [] }]) {
    const p = mergePrefs(junk);
    assert.deepEqual(p.order, KEYS);
    assert.deepEqual(p.sort, defaultPrefs().sort);
  }
  const p = mergePrefs({ widths: { title: 5, artist: 99999, bpm: "wide" }, rememberView: "yes", findClearsFilters: false, visible: { title: false } });
  assert.equal(p.widths.title, 90, "never narrower than the column's minimum");
  assert.equal(p.widths.artist, 800);
  assert.equal(p.widths.bpm, 64);
  assert.equal(p.rememberView, true, "a non-boolean setting keeps its default");
  assert.equal(p.findClearsFilters, false);
  assert.equal(p.visible.title, true, "a fixed column cannot be hidden");
});

test("sort lists: known fields, once each, valid directions", () => {
  assert.deepEqual(cleanSort([{ field: "bpm", asc: true }, { field: "bpm", asc: false }, { field: "nope", asc: true }, { field: "key", asc: "up" }, null, { field: "genre", asc: false }]), [
    { field: "bpm", asc: true },
    { field: "genre", asc: false },
  ]);
  assert.equal(cleanSort([]), null);
  assert.equal(cleanSort("bpm"), null);
  assert.equal(cleanSort([{ field: "wave", asc: true }]), null, "a column that does not sort is not a sort field");
  assert.deepEqual(mergePrefs({ sort: [{ field: "year", asc: false }, { field: "label", asc: true }] }).sort, [{ field: "year", asc: false }, { field: "label", asc: true }]);
});

test("column order: repeats and unknown keys dropped, missing columns slotted after their default neighbour, heart first", () => {
  const rev = [...KEYS].reverse();
  const o = cleanOrder(rev);
  assert.equal(o[0], "fav", "the heart is pinned first");
  assert.equal(new Set(o).size, KEYS.length);
  assert.deepEqual(cleanOrder(["title", "title", "nope", "artist"]).slice(0, 4), ["fav", "colour", "title", "artist"], "colour was missing, so it follows the heart");
  assert.deepEqual(cleanOrder("x"), KEYS);
  // an order saved before the new columns existed: they appear after the column before them
  const old = KEYS.filter((k) => !["year", "label", "remixer", "composer", "comments", "added", "lastplayed", "format", "bitrate", "size", "cases", "caseno"].includes(k));
  const swapped = ["fav", "colour", "artist", "title", ...old.slice(4)];
  const merged = cleanOrder(swapped);
  assert.equal(merged.length, KEYS.length);
  assert.deepEqual(merged.slice(0, 4), ["fav", "colour", "artist", "title"], "the saved order is kept");
  assert.equal(merged.indexOf("year"), merged.indexOf("tags") + 1, "a new column follows the column that precedes it by default");
  assert.equal(merged.indexOf("label"), merged.indexOf("year") + 1);
  assert.equal(merged.indexOf("cases"), merged.indexOf("size") + 1, "the case columns follow Size");
  assert.equal(merged.at(-1), "caseno");
});

test("moving a column: before another, to the end, never before the heart, the heart never moves", () => {
  const order = ["fav", "colour", "title", "artist", "album"];
  assert.deepEqual(moveColumn(order, "artist", "colour"), ["fav", "artist", "colour", "title", "album"]);
  assert.deepEqual(moveColumn(order, "colour", null), ["fav", "title", "artist", "album", "colour"]);
  assert.deepEqual(moveColumn(order, "album", "fav"), ["fav", "album", "colour", "title", "artist"], "dropped on the heart means just after it");
  assert.deepEqual(moveColumn(order, "fav", "album"), order);
  assert.deepEqual(moveColumn(order, "title", "title"), order);
  assert.deepEqual(moveColumn(order, "title", "ghost" ), ["fav", "colour", "artist", "album", "title"], "an unknown target means the end");
  assert.deepEqual(order, ["fav", "colour", "title", "artist", "album"], "the input is not changed");
});

test("keyboard moves among the shown columns", () => {
  const shown = ["fav", "title", "artist", "bpm"];
  assert.deepEqual(stepTarget(shown, "artist", -1), { ok: true, before: "title" });
  assert.deepEqual(stepTarget(shown, "title", -1), { ok: false, before: null }, "not before the heart");
  assert.deepEqual(stepTarget(shown, "artist", 1), { ok: true, before: null }, "past the last shown column means the end");
  assert.deepEqual(stepTarget(shown, "title", 1), { ok: true, before: "bpm" });
  assert.deepEqual(stepTarget(shown, "bpm", 1), { ok: false, before: null });
  assert.deepEqual(stepTarget(shown, "fav", 1), { ok: false, before: null });
  // hidden columns in between do not matter: the move is among the shown ones
  let order = ["fav", "title", "hiddenA", "artist", "hiddenB", "bpm"];
  const move = stepTarget(shown, "title", 1);
  order = moveColumn(order, "title", move.before);
  assert.deepEqual(order.filter((k) => shown.includes(k)), ["fav", "artist", "title", "bpm"]);
});

test("shown columns follow the saved order and leave out hidden ones", () => {
  const p = defaultPrefs();
  p.order = moveColumn(p.order, "bpm", "artist");
  p.visible.album = false;
  p.visible.year = true;
  const keys = shownColumns(p).map((c) => c.key);
  assert.equal(keys.indexOf("bpm") + 1, keys.indexOf("artist"));
  assert.ok(!keys.includes("album"));
  assert.ok(keys.includes("year"));
  assert.ok(keys.includes("title") && keys.includes("fav"), "fixed columns are always shown");
  p.visible.title = false;
  assert.ok(shownColumns(p).some((c) => c.key === "title"));
});

test("prefs survive a JSON round trip unchanged", () => {
  const p = defaultPrefs();
  p.order = moveColumn(p.order, "genre", "title");
  p.sort = [{ field: "key", asc: false }, { field: "bpm", asc: true }];
  p.rememberView = false;
  assert.deepEqual(mergePrefs(JSON.parse(JSON.stringify(p))), p);
});

test("the energy label and the auto energy estimate are two columns with two sort fields", () => {
  const label = COLUMNS.find((c) => c.key === "energy");
  const auto = COLUMNS.find((c) => c.key === "autoenergy");
  assert.ok(label && auto);
  assert.equal(label.sort, "energy");
  assert.equal(auto.sort, "auto_energy");
  assert.deepEqual(cleanSort([{ field: "auto_energy", asc: false }, { field: "energy", asc: true }]), [{ field: "auto_energy", asc: false }, { field: "energy", asc: true }]);
});
