// Tests for the pure track list settings helpers (tracklistprefs.ts). Plain node script, like the
// other tests of this project (`npm test`). Exits non-zero on the first failed assertion.
import assert from "node:assert/strict";
import * as P from "./tracklistprefs.ts";

let n = 0;
const test = (name, fn) => {
  fn();
  n++;
  console.log(`ok  ${name}`);
};

test("settings fall back to the defaults for anything missing or wrong", () => {
  assert.deepEqual(P.mergePrefs(null), P.defaultPrefs());
  assert.deepEqual(P.mergePrefs("x"), P.defaultPrefs());
  assert.deepEqual(P.mergePrefs({ options: 5, format: 3 }), P.defaultPrefs());
  const p = P.mergePrefs({ writeCue: true, format: "youtube", options: { stampAt: "crossfadeMid", rounding: "bogus", closeEntries: "drop", numbered: false, csvBom: "yes" } });
  assert.equal(p.writeCue, true);
  assert.equal(p.format, "youtube");
  assert.equal(p.options.stampAt, "crossfadeMid");
  assert.equal(p.options.rounding, "floor", "an unknown value is ignored");
  assert.equal(p.options.closeEntries, "drop");
  assert.equal(p.options.numbered, false);
  assert.equal(p.options.csvBom, true, "a non-boolean is ignored");
  assert.equal(p.options.includeUnavailable, false);
});

test("defaults are not shared between callers", () => {
  const a = P.defaultPrefs();
  a.options.numbered = false;
  assert.equal(P.defaultPrefs().options.numbered, true);
  assert.equal(P.DEFAULT_OPTIONS.numbered, true);
});

test("audio file names are safe on Windows", () => {
  assert.equal(P.audioFileName("My Mix"), "My Mix.ogg");
  assert.equal(P.audioFileName('a/b\\c:d*e?f"g<h>i|j', "wav"), "a b c d e f g h i j.wav");
  assert.equal(P.audioFileName("  Set... "), "Set.ogg");
  assert.equal(P.audioFileName("???"), "mix.ogg");
  assert.equal(P.audioFileName("tab\there\nline"), "tab here line.ogg");
});

test("extensions are added only when missing", () => {
  assert.equal(P.withExtension("C:\\x\\list", "cue"), "C:\\x\\list.cue");
  assert.equal(P.withExtension("list.CUE", "cue"), "list.CUE");
  assert.equal(P.withExtension("list.txt", "csv"), "list.txt.csv");
});

test("paths", () => {
  assert.equal(P.baseName("C:\\a\\b\\mix.ogg"), "mix.ogg");
  assert.equal(P.baseName("/a/b/mix.wav"), "mix.wav");
  assert.equal(P.cuePathFor("C:\\a.b\\mix.v2.ogg"), "C:\\a.b\\mix.v2.cue");
  assert.equal(P.cuePathFor("/a.b/noext"), "/a.b/noext.cue");
});

test("list file names", () => {
  assert.equal(P.listFileName("My Mix", "cue"), "My Mix.cue");
  assert.equal(P.listFileName("My Mix", "youtube"), "My Mix - YouTube chapters.txt");
  assert.equal(P.listFileName("My Mix", "csv"), "My Mix - CSV.csv");
});

console.log(`${n} tests passed`);
