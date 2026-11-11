// Run: node --experimental-strip-types --test src/lib/views/cases/caseLogic.test.mjs
// Pins the browser twins of the Rust merge / transfer plans to the same numbers the Rust tests use
// (crates/pm-formats/src/library/case_bulk_tests.rs), and tests the case table's pure logic.
import assert from "node:assert/strict";
import { test } from "node:test";
import { parseTrackIds, planMerge, planTransfer } from "../../state/casePlan.ts";
import { sortTracks } from "../tracks/trackLogic.ts";
import {
  CASE_COLUMNS, copyName, defaultCasePrefs, mergeCasePrefs, mergeSummary, plural, searchRows, sortRows, trackLabel, transferLine,
} from "./caseLogic.ts";

const t = (id, title, extra = {}) => ({
  track_id: id, title, artist: "Synthetic Artist", album: "", genre: "", key: "", bpm: 0, rating: 0, times_played: 0, play_time_secs: 100, ...extra,
});
const rowsOf = (...tracks) => tracks.map((x) => ({ id: x.track_id, t: x }));
const ids = (rows) => rows.map((r) => r.id);
const sorter = (rows, field, asc, fav) => sortTracks(rows, field, asc, fav);

test("merge order: first source's order, then new tracks of the next; every skip is counted", () => {
  const plan = planMerge([[1, 2, 9], [2, 3, 1]], [9]);
  assert.deepEqual(plan.added, [1, 2, 3]);
  assert.deepEqual([plan.already_there, plan.repeated, plan.stale], [1, 2, 0]);
  const fresh = planMerge([[5, 6], [6, 7]], []);
  assert.deepEqual(fresh.added, [5, 6, 7]);
  assert.deepEqual([fresh.already_there, fresh.repeated], [0, 1]);
  assert.deepEqual(planMerge([], [1]).added, []);
});

test("transfer plan: add, already there, remove, not in source, missing", () => {
  const known = (id) => id !== 99;
  const p = planTransfer([1, 2, 3, 2, 99, 7], [1, 2, 3, 4], [2], true, known);
  assert.deepEqual(p.to_add, [1, 3]);
  assert.deepEqual(p.already_there, [2]);
  assert.deepEqual(p.to_remove, [1, 2, 3], "a move removes it even when the destination had it");
  assert.deepEqual(p.not_in_source, [7]);
  assert.deepEqual(p.missing, [99]);
  const copy = planTransfer([1, 2], [1, 2], [], false, known);
  assert.deepEqual([copy.to_add, copy.to_remove], [[1, 2], []]);
  assert.deepEqual(planTransfer([7], null, [], false, known).to_add, [7]);
});

test("drag payloads: only integer arrays are accepted, repeats dropped", () => {
  assert.deepEqual(parseTrackIds("[3,1,3,2]"), [3, 1, 2]);
  for (const bad of ["", "nope", "{}", "\"3\"", "null"]) assert.deepEqual(parseTrackIds(bad), [], bad);
  assert.deepEqual(parseTrackIds("[1,\"2\",2.5,null,4]"), [1, 4]);
});

test("the case table offers the Tracks view's columns; title and heart are always on", () => {
  const keys = CASE_COLUMNS.map((c) => c.key);
  for (const k of ["title", "artist", "album", "genre", "wave", "bpm", "key", "rating", "plays", "time"]) assert.ok(keys.includes(k), k);
  const prefs = defaultCasePrefs();
  assert.ok(prefs.visible.title && prefs.visible.fav);
  assert.equal(prefs.sortKey, "order");
  assert.equal(prefs.mergeDeletesSources, false, "merging never deletes unless the user turns it on");
});

test("saved prefs: unknown and malformed values are ignored, fixed columns cannot be hidden", () => {
  assert.deepEqual(mergeCasePrefs(null), defaultCasePrefs());
  assert.deepEqual(mergeCasePrefs("x"), defaultCasePrefs());
  const p = mergeCasePrefs({ visible: { album: true, title: false, bogus: true, bpm: "no" }, sortKey: "bpm", sortAsc: false, mergeDeletesSources: true });
  assert.equal(p.visible.album, true);
  assert.equal(p.visible.title, true);
  assert.equal(p.visible.bpm, true);
  assert.equal("bogus" in p.visible, false);
  assert.deepEqual([p.sortKey, p.sortAsc, p.mergeDeletesSources], ["bpm", false, true]);
  assert.equal(mergeCasePrefs({ sortKey: "nope" }).sortKey, "order");
  assert.equal(mergeCasePrefs({ sortKey: "order", sortAsc: "x" }).sortAsc, true);
});

test("sorting: case order is kept or reversed; other keys sort the tracks; gone tracks stay last", () => {
  const rows = [...rowsOf(t(1, "Charlie", { bpm: 128 }), t(2, "alpha", { bpm: 120 }), t(3, "Bravo", { bpm: 124 })), { id: 77, t: undefined }];
  const fav = () => false;
  assert.deepEqual(ids(sortRows(rows, "order", true, sorter, fav)), [1, 2, 3, 77]);
  assert.deepEqual(ids(sortRows(rows, "order", false, sorter, fav)), [77, 3, 2, 1]);
  assert.deepEqual(ids(sortRows(rows, "title", true, sorter, fav)), [2, 3, 1, 77], "case-insensitive, like the Tracks view");
  assert.deepEqual(ids(sortRows(rows, "title", false, sorter, fav)), [1, 3, 2, 77], "a track that is gone stays last when descending too");
  assert.deepEqual(ids(sortRows(rows, "bpm", true, sorter, fav)), [2, 3, 1, 77]);
  assert.deepEqual(ids(sortRows(rows, "fav", true, sorter, (id) => id === 3)), [2, 1, 3, 77]);
  assert.deepEqual(ids(sortRows(rows, "fav", false, sorter, (id) => id === 3)), [3, 2, 1, 77], "favourites first when descending");
  assert.deepEqual(ids(rows), [1, 2, 3, 77], "the input is not modified");
});

test("search matches title, artist, album, genre and key; rows without a track never match", () => {
  const rows = [...rowsOf(t(1, "One", { genre: "House" }), t(2, "Two", { key: "8A" }), t(3, "Three", { album: "Synthetic Album" })), { id: 9, t: undefined }];
  assert.deepEqual(ids(searchRows(rows, "  ")), [1, 2, 3, 9]);
  assert.deepEqual(ids(searchRows(rows, "house")), [1]);
  assert.deepEqual(ids(searchRows(rows, "8a")), [2]);
  assert.deepEqual(ids(searchRows(rows, "synthetic album")), [3]);
  assert.deepEqual(ids(searchRows(rows, "synthetic artist")), [1, 2, 3]);
});

test("copy names: first free 'copy', 'copy 2', ... and the 255 limit holds", () => {
  assert.equal(copyName("Set A", []), "Set A copy");
  assert.equal(copyName("Set A", ["Set A copy"]), "Set A copy 2");
  assert.equal(copyName("Set A", ["set a COPY", "Set A copy 2"]), "Set A copy 3");
  const long = copyName("x".repeat(255), []);
  assert.equal([...long].length, 255);
  assert.ok(long.endsWith(" copy"));
});

test("sentences name what happens", () => {
  assert.equal(plural(1, "track"), "1 track");
  assert.equal(plural(2, "track"), "2 tracks");
  const s = mergeSummary({ added: [1, 2], already_there: 1, repeated: 2, stale: 1 }, "Dest");
  assert.match(s, /2 tracks will be added to “Dest”/);
  assert.match(s, /1 track is already in it/);
  assert.match(s, /2 tracks appear in more than one/);
  assert.match(s, /1 entry in the cases points at tracks no longer in the library and is left out/);
  assert.equal(mergeSummary({ added: [], already_there: 0, repeated: 0, stale: 0 }, "D"), "0 tracks will be added to “D”.");

  const plan = planTransfer([1, 2, 3, 7, 99], [1, 2, 3], [2], true, (id) => id !== 99);
  assert.equal(transferLine(1, plan, "To", "From", true), "will move from “From” to “To”");
  assert.equal(transferLine(2, plan, "To", "From", true), "is already in “To”: only removed from “From”");
  assert.equal(transferLine(7, plan, "To", "From", true), "is not in “From”: left alone");
  assert.equal(transferLine(99, plan, "To", "From", true), "is not in the library: left alone");
  const copy = planTransfer([1, 2], [1, 2], [2], false, () => true);
  assert.equal(transferLine(1, copy, "To", "From", false), "will be added to “To”");
  assert.equal(transferLine(2, copy, "To", "From", false), "is already in “To”: nothing to do");
  assert.equal(trackLabel(5, undefined), "track #5 (no longer in the library)");
  assert.equal(trackLabel(5, t(5, "Song")), "Song by Synthetic Artist");
});
