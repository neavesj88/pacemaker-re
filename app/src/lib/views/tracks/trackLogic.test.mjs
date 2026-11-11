// Run: node --experimental-strip-types --test src/lib/views/tracks/trackLogic.test.mjs
import assert from "node:assert/strict";
import { test } from "node:test";
import { LABEL_COLOURS } from "../../state/labels.ts";
import {
  COLOUR_SORT_ORDER, filterTracks, formatDate, formatSize, keyName, keysCompatible, matchingTracks, NO_FILTER, parseBpm, parseKey, sortClick, sortTracks, sortTracksBy, uniqueGenres,
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
  rating: 0, location: "", file_size: 0, format: "MP3", analysed: true, times_played: 0, last_played: -1,
  year: 0, label: "", remixer: "", composer: "", comments: "", date_added: -1, bit_rate: 0, ...over,
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

test("sorting by the app-side labels: colour palette order, energy, tags; unlabelled tracks last either way", () => {
  const list = [T(1), T(2), T(3), T(4), T(5)];
  const labels = {
    1: { tags: ["b"], colour: "blue", energy: 5 },
    2: { tags: [], colour: "red", energy: 9 },
    3: { tags: ["A", "z"], colour: "pink", energy: null },
    4: { tags: ["a"], colour: null, energy: 1 },
  };
  const ids = (f, asc = true) => sortTracks(list, f, asc, () => false, (id) => labels[id]).map((t) => t.track_id);
  assert.deepEqual(ids("colour"), [2, 1, 3, 4, 5]);
  assert.deepEqual(ids("colour", false), [3, 1, 2, 4, 5]);
  assert.deepEqual(ids("energy"), [4, 1, 2, 3, 5]);
  assert.deepEqual(ids("energy", false), [2, 1, 4, 3, 5]);
  assert.deepEqual(ids("tags"), [4, 3, 1, 2, 5]);
  assert.deepEqual(ids("tags", false), [1, 3, 4, 2, 5]);
  // without a label source every track is unlabelled and the order falls back to title
  assert.deepEqual(sortTracks(list, "energy", false, () => false).map((t) => t.track_id), [1, 2, 3, 4, 5]);
});

test("the colour sort order is the label palette", () => {
  assert.deepEqual([...COLOUR_SORT_ORDER], [...LABEL_COLOURS]);
});

test("multi-key sort: later keys break ties, each key has its own direction, the title is the last resort", () => {
  const list = [
    T(1, { title: "d", genre: "House", bpm: 128 }), T(2, { title: "c", genre: "Techno", bpm: 128 }),
    T(3, { title: "b", genre: "House", bpm: 124 }), T(4, { title: "a", genre: "House", bpm: 128 }),
  ];
  const ids = (keys) => sortTracksBy(list, keys, () => false).map((t) => t.track_id);
  assert.deepEqual(ids([{ field: "genre", asc: true }, { field: "bpm", asc: false }]), [4, 1, 3, 2]);
  assert.deepEqual(ids([{ field: "genre", asc: true }, { field: "bpm", asc: true }]), [3, 4, 1, 2]);
  assert.deepEqual(ids([{ field: "genre", asc: false }, { field: "bpm", asc: true }]), [2, 3, 4, 1]);
  assert.deepEqual(ids([{ field: "bpm", asc: true }]), [3, 4, 2, 1], "ties fall to the title, whichever way the key runs");
  assert.deepEqual(ids([{ field: "bpm", asc: false }]), [4, 2, 1, 3]);
  assert.deepEqual(ids([]), [4, 3, 2, 1], "no keys: by title");
  assert.deepEqual(sortTracksBy(list, [{ field: "genre", asc: true }, { field: "bpm", asc: false }], () => false).map((t) => t.track_id), sortTracksBy(list, [{ field: "genre", asc: true }, { field: "bpm", asc: false }], () => false).map((t) => t.track_id), "stable");
});

test("columns with nothing in them sort last in both directions (year, date added, last played, bit rate, label, remixer, composer, comments, format)", () => {
  const list = [
    T(1, { year: 2012, date_added: 3000, last_played: 5000, bit_rate: 320, label: "B", remixer: "r2", composer: "c2", comments: "z", format: "WAV" }),
    T(2, { format: "" }),
    T(3, { year: 1999, date_added: 1000, last_played: 2000, bit_rate: 128, label: "a", remixer: "r1", composer: "c1", comments: "y", format: "MP3" }),
    T(4, { label: "  ", format: " " }),
  ];
  for (const field of ["year", "date_added", "last_played", "bit_rate", "label", "remixer", "composer", "comments", "format"]) {
    const asc = sortTracks(list, field, true, () => false).map((t) => t.track_id);
    const desc = sortTracks(list, field, false, () => false).map((t) => t.track_id);
    assert.deepEqual(asc, [3, 1, 2, 4], field + " ascending");
    assert.deepEqual(desc, [1, 3, 2, 4], field + " descending");
  }
});

test("file size and the existing number columns sort numerically", () => {
  const list = [T(1, { file_size: 900 }), T(2, { file_size: 10_000 }), T(3, { file_size: 50 })];
  assert.deepEqual(sortTracks(list, "file_size", true, () => false).map((t) => t.track_id), [3, 1, 2]);
  assert.deepEqual(sortTracks(list, "file_size", false, () => false).map((t) => t.track_id), [2, 1, 3]);
});

test("header clicks: click = the only key (or reverse the primary and keep the others); shift-click adds, reverses, removes", () => {
  const A = (field) => ({ field, asc: true });
  const D = (field) => ({ field, asc: false });
  // plain click
  assert.deepEqual(sortClick([A("artist")], "title", false), [A("title")]);
  assert.deepEqual(sortClick([A("artist"), A("bpm")], "title", false), [A("title")], "another column replaces the whole sort");
  assert.deepEqual(sortClick([A("artist")], "artist", false), [D("artist")]);
  assert.deepEqual(sortClick([D("artist")], "artist", false), [A("artist")]);
  assert.deepEqual(sortClick([A("artist"), D("bpm")], "artist", false), [D("artist"), D("bpm")], "reversing the primary keeps the secondary keys");
  assert.deepEqual(sortClick([A("artist"), D("bpm")], "bpm", false), [A("bpm")], "a secondary key clicked plainly becomes the only key");
  // shift-click
  assert.deepEqual(sortClick([A("artist")], "bpm", true), [A("artist"), A("bpm")]);
  assert.deepEqual(sortClick([A("artist"), A("bpm")], "bpm", true), [A("artist"), D("bpm")]);
  assert.deepEqual(sortClick([A("artist"), D("bpm")], "bpm", true), [A("artist")], "the third shift-click removes it");
  assert.deepEqual(sortClick([A("artist"), D("bpm")], "artist", true), [D("artist"), D("bpm")]);
  assert.deepEqual(sortClick([D("artist"), D("bpm")], "artist", true), [D("bpm")], "removing the primary promotes the next key");
  assert.deepEqual(sortClick([D("artist")], "artist", true), [A("artist")], "a lone key is reversed, never removed");
  const before = [A("artist")];
  sortClick(before, "bpm", true);
  assert.deepEqual(before, [A("artist")], "the input is not changed");
});

test("cell text for dates and sizes", () => {
  assert.equal(formatDate(new Date(2026, 2, 4, 23, 59).getTime()), "2026-03-04");
  assert.equal(formatDate(-1), "");
  assert.equal(formatDate(0), "");
  assert.equal(formatSize(0), "");
  assert.equal(formatSize(512), "512 B");
  assert.equal(formatSize(1536), "1.5 KB");
  assert.equal(formatSize(8 * 1024 * 1024 + 400_000), "8.4 MB");
  assert.equal(formatSize(150 * 1024 * 1024), "150 MB");
  assert.equal(formatSize(3 * 1024 ** 3), "3.0 GB");
});
