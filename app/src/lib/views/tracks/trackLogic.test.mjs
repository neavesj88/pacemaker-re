// Run: node --experimental-strip-types --test src/lib/views/tracks/trackLogic.test.mjs
import assert from "node:assert/strict";
import { test } from "node:test";
import {
  filterTracks, keyName, keysCompatible, matchingTracks, NO_FILTER, parseBpm, parseKey, sortTracks, uniqueGenres,
} from "./trackLogic.ts";

const code = (k) => parseKey(k)?.code ?? null;

test("stored keys map to Camelot", () => {
  const cases = {
    C: "8B", Am: "8A", "A minor": "8A", G: "9B", Em: "9A", "F#m": "11A", "Ab Minor": "1A", Abm: "1A", "G#m": "1A",
    Db: "3B", "C#": "3B", Bbm: "3A", Bb: "6B", F: "7B", Dm: "7A", B: "1B", "B major": "1B", Bm: "10A", Bmin: "10A",
    "F# major": "2B", Eb: "5B", Cm: "5A", "C♯m": "12A", "E♭": "5B", "8a": "8A", "12B": "12B", " Am ": "8A",
  };
  for (const [k, c] of Object.entries(cases)) assert.equal(code(k), c, k);
});

test("empty and unknown keys give null", () => {
  for (const k of ["", "  ", "H", "Xm", "Cdim", "13A", "0B", "Am7"]) assert.equal(code(k), null, JSON.stringify(k));
});

test("key names read back to the same Camelot key", () => {
  for (const k of ["Am", "Cm", "F#m", "C", "Eb", "Bbm", "F#"]) {
    const c = parseKey(k);
    assert.equal(parseKey(keyName(c))?.code, c.code, k);
  }
  assert.equal(keyName(parseKey("8A")), "A minor");
  assert.equal(keyName(parseKey("8B")), "C major");
});

test("compatibility: same, one step round the wheel (wrapping), relative; nothing else", () => {
  const k = (c) => parseKey(c);
  assert.ok(keysCompatible(k("8A"), k("8A")));
  assert.ok(keysCompatible(k("8A"), k("9A")));
  assert.ok(keysCompatible(k("8A"), k("7A")));
  assert.ok(keysCompatible(k("8A"), k("8B")));
  assert.ok(keysCompatible(k("12A"), k("1A")));
  assert.ok(keysCompatible(k("1B"), k("12B")));
  assert.ok(!keysCompatible(k("8A"), k("9B")));
  assert.ok(!keysCompatible(k("8A"), k("10A")));
  assert.ok(!keysCompatible(k("1A"), k("11A")));
});

const T = (id, over = {}) => ({
  track_id: id, title: `T${id}`, artist: "", album: "", genre: "House", bpm: 128, key: "Am", play_time_secs: 200,
  rating: 0, location: "", file_size: 0, format: "MP3", analysed: true, times_played: 0, ...over,
});

test("filters: genre, bpm range, key, analysed", () => {
  const list = [
    T(1), T(2, { genre: "Techno", bpm: 140 }), T(3, { key: "" }), T(4, { bpm: 0, key: "Em" }), T(5, { key: "C" }), T(6, { key: "Bm" }),
  ];
  const needs = new Map([[5, "no .str file"]]);
  const ids = (f) => filterTracks(list, { ...NO_FILTER, ...f }, needs).map((t) => t.track_id);
  assert.deepEqual(ids({}), [1, 2, 3, 4, 5, 6]);
  assert.deepEqual(ids({ genre: "Techno" }), [2]);
  assert.deepEqual(ids({ bpmMin: 130 }), [2]);
  assert.deepEqual(ids({ bpmMax: 130 }), [1, 3, 6]); // 5 needs analysis: shown without a BPM
  assert.deepEqual(ids({ bpmMin: 120, bpmMax: 130 }), [1, 3, 6]);
  assert.deepEqual(ids({ key: "compat:8A" }), [1, 2, 4, 5]);
  assert.deepEqual(ids({ key: "is:8A" }), [1, 2]);
  assert.deepEqual(ids({ onlyAnalysed: true }), [1, 2, 3, 4, 6]);
});

test("parseBpm", () => {
  assert.equal(parseBpm("128"), 128);
  assert.equal(parseBpm(" 127,5 "), 127.5);
  for (const t of ["", " ", "abc", "0", "-3"]) assert.equal(parseBpm(t), null, t);
});

test("sorting: text, numbers, Camelot order, favourites, stable ties, direction", () => {
  const list = [
    T(1, { title: "b", key: "C", bpm: 130, rating: 2 }), T(2, { title: "A", key: "Am", bpm: 120, rating: 5 }),
    T(3, { title: "c", key: "", bpm: 125, rating: 2 }), T(4, { title: "a", key: "Em", bpm: 125, rating: 0 }),
  ];
  const ids = (f, asc = true, fav = () => false) => sortTracks(list, f, asc, fav).map((t) => t.track_id);
  assert.deepEqual(ids("title"), [2, 4, 1, 3]);
  assert.deepEqual(ids("title", false), [3, 1, 2, 4]);
  assert.deepEqual(ids("bpm"), [2, 4, 3, 1]);
  assert.deepEqual(ids("bpm", false), [1, 4, 3, 2]);
  assert.deepEqual(ids("key"), [2, 1, 4, 3]);
  assert.deepEqual(ids("rating"), [4, 1, 3, 2]);
  assert.deepEqual(ids("fav", false, (id) => id === 3), [3, 2, 4, 1]);
  assert.equal(list[0].track_id, 1, "input is not mutated");
});

test("sorting by auto energy: numbers in either direction, tracks without one always last", () => {
  const list = [T(1, { title: "b" }), T(2, { title: "a" }), T(3, { title: "c" }), T(4, { title: "d" }), T(5, { title: "e" })];
  const energy = new Map([[1, 7.4], [2, 3.0], [4, 7.4]]);
  const of = (id) => energy.get(id) ?? null;
  const ids = (asc) => sortTracks(list, "energy", asc, () => false, of).map((t) => t.track_id);
  assert.deepEqual(ids(true), [2, 1, 4, 3, 5], "ties by title, then the unanalysed by title");
  assert.deepEqual(ids(false), [1, 4, 2, 3, 5], "descending keeps the blanks at the end");
  assert.deepEqual(sortTracks(list, "energy", true, () => false).map((t) => t.track_id), [2, 1, 3, 4, 5], "with no energy at all it is a title sort");
  assert.equal(list[0].track_id, 1, "input is not mutated");
});

test("matching tracks: compatible key and bpm within 4 percent, nearest bpm first, excludes itself", () => {
  const focus = T(1, { bpm: 128, key: "Am" });
  const list = [
    focus, T(2, { bpm: 130, key: "Em" }), T(3, { bpm: 133.2, key: "Am" }), T(4, { bpm: 126, key: "C" }),
    T(5, { bpm: 128, key: "F#m" }), T(6, { bpm: 129, key: "" }), T(7, { bpm: 127.9, key: "Dm" }),
  ];
  assert.deepEqual(matchingTracks(focus, list).map((t) => t.track_id), [7, 2, 4]);
  assert.deepEqual(matchingTracks(T(9, { key: "" }), list), []);
  assert.deepEqual(matchingTracks(T(9, { bpm: 0 }), list), []);
  assert.deepEqual(matchingTracks(focus, list, 4, new Map([[7, "no .str file"]])).map((t) => t.track_id), [2, 4], "tracks needing analysis show no BPM, so they are not matched");
});

test("unique genres are sorted and non-empty", () => {
  assert.deepEqual(uniqueGenres([T(1, { genre: "Techno" }), T(2, { genre: "" }), T(3), T(4, { genre: "house" })]), ["House", "house", "Techno"]);
});
