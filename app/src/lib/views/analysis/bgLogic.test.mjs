// Run: node --experimental-strip-types --test src/lib/views/analysis/bgLogic.test.mjs
import assert from "node:assert/strict";
import { test } from "node:test";
import { clampPct, failureNote, failureRows, groupKeys, libraryChanged, pauseAction, statusLine, stripVisible } from "./bgLogic.ts";

const base = { state: "running", detail: "", root: "C:\\Lib", pacemaker: false, word: "waveforms", done: 12, total: 40, left: 100, label: "A - B", failures: 0 };
const fail = (kind, track_id, reason, label = `Track ${track_id}`) => ({ kind, track_id, label, location: `/pmdb_tracks/${track_id}.mp3`, size: 1, reason });

test("the running line names the lane and its count", () => {
  assert.equal(statusLine(base), "Analysing in the background: 12 of 40 - waveforms");
  assert.match(statusLine({ ...base, word: "", total: 0 }), /checking what is missing/);
});

test("the strip shows while work runs or waits with something left, and never when there is nothing to say", () => {
  assert.ok(stripVisible(base));
  assert.ok(stripVisible({ ...base, state: "paused" }));
  assert.ok(!stripVisible({ ...base, state: "paused", left: 0 }));
  assert.ok(stripVisible({ ...base, state: "blocked", detail: "the library is being backed up" }));
  for (const state of ["idle", "off", "manual", "no-library"]) assert.ok(!stripVisible({ ...base, state }), state);
});

test("waiting states say why and how much is left", () => {
  assert.match(statusLine({ ...base, state: "playing" }), /while a preview plays \(100 left in all\)/);
  assert.match(statusLine({ ...base, state: "waiting" }), /waits for the job you started/);
  assert.match(statusLine({ ...base, state: "paused", left: 0 }), /^Background analysis paused$/);
  assert.match(statusLine({ ...base, state: "blocked", detail: "the device is being ejected" }), /held up: the device is being ejected\. It tries again/);
  assert.match(statusLine({ ...base, state: "error", detail: "music.db is locked" }), /could not read the library: music\.db is locked/);
});

test("the idle line counts what it gave up on", () => {
  assert.match(statusLine({ ...base, state: "idle", failures: 0 }), /nothing is missing/);
  assert.match(statusLine({ ...base, state: "idle", failures: 1 }), /except 1 result it could not make/);
  assert.match(statusLine({ ...base, state: "idle", failures: 3 }), /except 3 results/);
  assert.match(statusLine({ ...base, state: "manual" }), /never starts on a Pacemaker by itself/);
});

test("pause and resume appear only when they mean something", () => {
  assert.equal(pauseAction(base), "pause");
  assert.equal(pauseAction({ ...base, state: "paused" }), "resume");
  assert.equal(pauseAction({ ...base, state: "idle" }), null);
  assert.equal(pauseAction({ ...base, state: "off" }), null);
});

test("finished keys are grouped by the key so the table is patched once per key", () => {
  const g = groupKeys([{ track_id: 1, key: "Am" }, { track_id: 2, key: "C" }, { track_id: 3, key: "Am" }]);
  assert.deepEqual([...g], [["Am", [1, 3]], ["C", [2]]]);
  assert.equal(groupKeys([]).size, 0);
});

test("the failure list is one row per track with every kind and reason", () => {
  const rows = failureRows([fail("keys", 2, "no clear key", "B track"), fail("peaks", 2, "cannot decode", "B track"), fail("energy", 1, "cannot decode", "A track"), fail("keys", 2, "no clear key", "B track")]);
  assert.deepEqual(rows, [
    { track_id: 1, label: "A track", kinds: "energy", reasons: "cannot decode" },
    { track_id: 2, label: "B track", kinds: "keys, waveforms", reasons: "no clear key; cannot decode" },
  ]);
});

test("the note counts tracks, not results, and stays away once dismissed until more fail", () => {
  assert.equal(failureNote([], 0), null);
  const two = [fail("keys", 2, "x"), fail("peaks", 2, "y")];
  assert.equal(failureNote(two, 0), "1 track couldn't be analysed.");
  assert.equal(failureNote(two, 2), null);
  assert.equal(failureNote([...two, fail("keys", 3, "z")], 2), "2 tracks couldn't be analysed.");
});

test("the key floor is clamped to whole per cent between 0 and 100", () => {
  assert.equal(clampPct(NaN), 0);
  assert.equal(clampPct(-5), 0);
  assert.equal(clampPct(150), 100);
  assert.equal(clampPct(42.6), 43);
});

test("the failure list and its Hide belong to one library: another root, or none, is a change", () => {
  assert.equal(libraryChanged("C:/Lib", "C:/Other"), true, "two libraries with the same failure count are still two");
  assert.equal(libraryChanged("C:/Lib", "c:/lib/"), false, "one library, two spellings");
  assert.equal(libraryChanged(null, "C:/Lib"), true);
  assert.equal(libraryChanged("C:/Lib", null), true);
  assert.equal(libraryChanged(null, null), false);
  assert.equal(libraryChanged(undefined, null), false);
});
