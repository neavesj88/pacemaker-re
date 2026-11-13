// Run: node --experimental-strip-types --test src/lib/views/insights/gapLogic.test.mjs
import assert from "node:assert/strict";
import { test } from "node:test";
import {
  anyEnergy, bandIndex, buildBpmGenre, buildBpmKey, buildKeyEnergy, buildMatrix, cellFilter, DEFAULT_SETTINGS, energyOf, hasBpm,
  KEY_ORDER, mergeSettings, settingNotice, summarise, thinCellSet,
} from "./gapLogic.ts";
import { filterTracks, parseKey } from "../tracks/trackLogic.ts";

const keyCode = (k) => parseKey(k)?.code ?? null;
const S = DEFAULT_SETTINGS;
const NONE = new Map();

let nextId = 1;
const track = (o) => ({
  track_id: nextId++, title: "t", artist: "a", album: "", genre: "House", bpm: 124, key: "Am", play_time_secs: 200, rating: 0,
  location: "", file_size: 1, format: "MP3", analysed: true, times_played: 0, ...o,
});
const many = (n, o) => Array.from({ length: n }, () => track(o));
const ids = (list) => list.map((t) => t.track_id).sort((a, b) => a - b);

test("bands are half-open: an edge belongs to the band it starts", () => {
  assert.equal(bandIndex(60, 60, 10), 0);
  assert.equal(bandIndex(69.999, 60, 10), 0);
  assert.equal(bandIndex(70, 60, 10), 1);
  assert.equal(bandIndex(128, 60, 10), 6);
  assert.equal(bandIndex(130, 60, 10), 7);
  assert.equal(bandIndex(129.99999999999997, 60, 10), 6);
  assert.equal(bandIndex(100, 60, 20), 2);
});

test("band edges survive division rounding for every width", () => {
  for (const w of [2, 5, 10, 20]) {
    for (let lo = 60; lo < 200; lo += w) {
      assert.equal(bandIndex(lo, 60, w), (lo - 60) / w, `${lo}/${w}`);
      assert.equal(bandIndex(lo - 1e-9, 60, w), (lo - 60) / w - 1, `below ${lo}/${w}`);
    }
  }
});

test("BPM x key: counts land in the right cell, rows span the occupied bands including empty ones", () => {
  const tracks = [
    ...many(3, { bpm: 124, key: "Am" }), // 120-130, 8A
    ...many(2, { bpm: 125, key: "C" }), // 120-130, 8B
    track({ bpm: 90, key: "Cm" }), // 90-100, 5A
    track({ bpm: 120, key: "8A" }), // edge: 120-130
    track({ bpm: 130, key: "8A" }), // edge: 130-140
  ];
  const m = buildBpmKey(tracks, NONE, S, keyCode);
  assert.deepEqual(m.rows.map((r) => r.label), ["90–100", "100–110", "110–120", "120–130", "130–140"]);
  assert.equal(m.cols.length, 24);
  const at = (band, code) => m.cells[m.rows.findIndex((r) => r.label === band)][KEY_ORDER.indexOf(code)];
  assert.equal(at("120–130", "8A").length, 4);
  assert.equal(at("120–130", "8B").length, 2);
  assert.equal(at("90–100", "5A").length, 1);
  assert.equal(at("130–140", "8A").length, 1);
  assert.deepEqual(m.rowTotals, [1, 0, 0, 6, 1]);
  assert.equal(m.total, 8);
  assert.equal(m.max, 4);
  assert.equal(m.colTotals[KEY_ORDER.indexOf("8A")], 5);
});

test("every track is placed or counted as left out, never both and never lost", () => {
  const tracks = [
    track({ bpm: 124, key: "Am" }),
    track({ bpm: 0, key: "Am" }), // no BPM
    track({ bpm: 124, key: "Am" }), // needs analysis (below)
    track({ bpm: 40, key: "Am" }), // outside the range
    track({ bpm: 200, key: "Am" }), // range is open at the top
    track({ bpm: 124, key: "" }), // no key
    track({ bpm: 124, key: "H#" }), // unreadable key
  ];
  const needs = new Map([[tracks[2].track_id, "never analysed"]]);
  const m = buildBpmKey(tracks, needs, S, keyCode);
  assert.equal(m.total, 1);
  const left = Object.fromEntries(m.skipped.map((s) => [s.reason, s.count]));
  assert.deepEqual(left, { "no BPM (not analysed yet)": 2, "BPM outside 60–200": 2, "no key": 2 });
  assert.equal(m.total + m.skipped.reduce((n, s) => n + s.count, 0), m.considered);
});

test("BPM x genre: biggest genres get rows, the rest fold into one, untagged tracks are counted out", () => {
  const tracks = [
    ...many(5, { genre: "House", bpm: 124 }),
    ...many(3, { genre: "Techno", bpm: 132 }),
    ...many(2, { genre: "Dub", bpm: 70 }),
    track({ genre: "Drone", bpm: 65 }),
    track({ genre: "", bpm: 124 }),
  ];
  const m = buildBpmGenre(tracks, NONE, { ...S, maxGenres: 3 });
  assert.deepEqual(m.rows.map((r) => r.label), ["House", "Techno", "Dub", "Other genres"]);
  assert.equal(m.rows[3].value.count, 1);
  assert.deepEqual(m.rowTotals, [5, 3, 2, 1]);
  assert.deepEqual(m.skipped, [{ reason: "no genre", count: 1 }]);
  assert.equal(m.colsOrdered, true);
  assert.equal(m.cols[0].label, "60–70");
  assert.equal(m.cols.at(-1).label, "130–140");
  // 4 bands between 70 and 120 are empty and shown
  assert.equal(m.cols.length, 8);
});

test("genre ties order by name so the grid does not shuffle between runs", () => {
  const tracks = [...many(2, { genre: "Zed" }), ...many(2, { genre: "Alpha" })];
  assert.deepEqual(buildBpmGenre(tracks, NONE, S).rows.map((r) => r.label), ["Alpha", "Zed"]);
});

test("key x energy: columns follow the scale in use, tracks without energy or key are counted out", () => {
  assert.equal(energyOf(track({ energy: 7.4 })), 7);
  assert.equal(energyOf(track({ energy: 0 })), null);
  assert.equal(energyOf(track({ energy: null })), null);
  assert.equal(energyOf(track({})), null);
  assert.equal(energyOf(track({ energy: NaN })), null);
  assert.equal(energyOf(track({ energy: 99 })), 10);
  assert.equal(anyEnergy([track({}), track({ energy: 3 })]), true);
  assert.equal(anyEnergy([track({}), track({ energy: 0 })]), false);

  const five = buildKeyEnergy([track({ energy: 5, key: "Am" }), track({ energy: 2, key: "Am" }), track({ key: "Am" })], keyCode);
  assert.equal(five.cols.length, 5);
  assert.equal(five.total, 2);
  assert.deepEqual(five.skipped, [{ reason: "no energy", count: 1 }]);
  const ten = buildKeyEnergy([track({ energy: 9, key: "C" }), track({ energy: 1, key: "" })], keyCode);
  assert.equal(ten.cols.length, 10);
  assert.equal(ten.cells[KEY_ORDER.indexOf("8B")][8].length, 1);
  assert.deepEqual(ten.skipped, [{ reason: "no key", count: 1 }]);
});

test("empty library gives empty grids without throwing", () => {
  for (const id of ["bpm-key", "bpm-genre", "key-energy"]) {
    const m = buildMatrix(id, [], NONE, S, keyCode);
    assert.equal(m.total, 0);
    assert.equal(m.max, 0);
    const sum = summarise(m, S);
    assert.deepEqual([sum.thinRows, sum.emptyRows, sum.emptyCols, sum.thinCols, sum.busiest], [[], [], [], [], null]);
  }
});

test("thin areas: only rows big enough are judged, empty and few are told apart", () => {
  const tracks = [
    ...many(6, { bpm: 124, key: "Am" }),
    ...many(3, { bpm: 126, key: "C" }),
    track({ bpm: 127, key: "Dm" }), // row 120-130 has 10
    ...many(4, { bpm: 95, key: "Am" }), // too small to judge
  ];
  const m = buildBpmKey(tracks, NONE, { ...S, thinBelow: 3 }, keyCode);
  const sum = summarise(m, { thinBelow: 3, minRowTracks: 10 });
  assert.equal(sum.thinRows.length, 1);
  const row = sum.thinRows[0];
  assert.equal(m.rows[row.row].label, "120–130");
  assert.equal(row.total, 10);
  assert.equal(row.empty.length, 24 - 3);
  assert.deepEqual(row.few.map((f) => [KEY_ORDER[f.col], f.count]), [["7A", 1]]);
  assert.ok(!row.empty.includes(KEY_ORDER.indexOf("8A")));
  // 100-110 and 110-120 hold nothing and sit between populated bands
  assert.deepEqual(sum.emptyRows.map((i) => m.rows[i].label), ["100–110", "110–120"]);
  // keys with fewer than 3 tracks in the whole grid include 5A (never used)
  assert.ok(sum.thinCols.some((c) => KEY_ORDER[c.col] === "5A" && c.total === 0));
  assert.ok(!sum.thinCols.some((c) => KEY_ORDER[c.col] === "8A"));
  assert.deepEqual(sum.busiest && [m.rows[sum.busiest.row].label, KEY_ORDER[sum.busiest.col], sum.busiest.count], ["120–130", "8A", 6]);
  const marks = thinCellSet(sum);
  assert.equal(marks.size, row.empty.length + row.few.length);
  assert.ok(marks.has(`${row.row}:${KEY_ORDER.indexOf("5A")}`));
});

test("thin areas on an ordered axis ignore the stretch outside the row's own range", () => {
  // Techno lives at 130-160 and has nothing at 140-150, which Dub (at 140-150) does fill.
  // Nothing at 60-120 is a gap for Techno; 140-150 is.
  const tracks = [
    ...many(6, { genre: "Techno", bpm: 132 }),
    ...many(5, { genre: "Techno", bpm: 152 }),
    ...many(11, { genre: "Dub", bpm: 144 }),
  ];
  const m = buildBpmGenre(tracks, NONE, S);
  const sum = summarise(m, { thinBelow: 2, minRowTracks: 10 });
  const techno = sum.thinRows.find((t) => m.rows[t.row].label === "Techno");
  assert.deepEqual(techno.empty.map((c) => m.cols[c].label), ["140–150"]);
  assert.ok(!sum.thinRows.some((t) => m.rows[t.row].label === "Dub" && t.empty.length > 0 && t.empty.some((c) => m.cols[c].label === "60–70")));
  assert.deepEqual(sum.emptyCols, []);
});

test("a band empty for every row is reported once, not under each row", () => {
  const tracks = [...many(6, { genre: "Techno", bpm: 132 }), ...many(5, { genre: "Techno", bpm: 152 })];
  const m = buildBpmGenre(tracks, NONE, S);
  const sum = summarise(m, { thinBelow: 2, minRowTracks: 10 });
  assert.deepEqual(sum.emptyCols.map((c) => m.cols[c].label), ["140–150"]);
  assert.equal(sum.thinRows.length, 0);
});

test("a full row has no thin cells", () => {
  const tracks = KEY_ORDER.flatMap((k) => many(2, { bpm: 124, key: k }));
  const m = buildBpmKey(tracks, NONE, S, keyCode);
  assert.equal(summarise(m, { thinBelow: 2, minRowTracks: 10 }).thinRows.length, 0);
});

test("settings: defaults for junk, clamped, range holds whole bands", () => {
  assert.deepEqual(mergeSettings(null), DEFAULT_SETTINGS);
  assert.deepEqual(mergeSettings("x"), DEFAULT_SETTINGS);
  assert.deepEqual(mergeSettings({ bandWidth: 7 }), DEFAULT_SETTINGS);
  const s = mergeSettings({ bandWidth: 20, bpmMin: 65, bpmMax: 190, maxGenres: 1000, thinBelow: 0, minRowTracks: -4 });
  assert.equal(s.bandWidth, 20);
  assert.equal(s.bpmMin, 65);
  assert.equal(s.bpmMax, 65 + 6 * 20); // 185: whole bands only
  assert.equal(s.maxGenres, 40);
  assert.equal(s.thinBelow, 1);
  assert.equal(s.minRowTracks, 1);
  // a range narrower than one band keeps the bound that was not just edited
  const base = { bandWidth: 20, bpmMin: 100, bpmMax: 200 };
  const lowerMax = mergeSettings({ ...base, bpmMax: 110 }, "bpmMax");
  assert.deepEqual([lowerMax.bpmMin, lowerMax.bpmMax], [100, 120]);
  const raiseMin = mergeSettings({ ...base, bpmMin: 195 }, "bpmMin");
  assert.deepEqual([raiseMin.bpmMin, raiseMin.bpmMax], [180, 200]);
  const lowMax = mergeSettings({ bandWidth: 20, bpmMin: 150, bpmMax: 30 }, "bpmMax");
  assert.deepEqual([lowMax.bpmMin, lowMax.bpmMax], [150, 170]);
  const lowMin = mergeSettings({ bandWidth: 20, bpmMin: 300, bpmMax: 30 }, "bpmMin");
  assert.deepEqual([lowMin.bpmMin, lowMin.bpmMax], [20, 40]);
  // not an edit of either bound (saved settings, a wider band): the min stays, the max moves
  const wider = mergeSettings({ bandWidth: 20, bpmMin: 100, bpmMax: 110 });
  assert.deepEqual([wider.bpmMin, wider.bpmMax], [100, 120]);
  assert.deepEqual(mergeSettings(mergeSettings({ bandWidth: 5, bpmMin: 71, bpmMax: 188 })), mergeSettings({ bandWidth: 5, bpmMin: 71, bpmMax: 188 }));
});

test("settings notice: says so when a value was not used as typed", () => {
  const D = DEFAULT_SETTINGS;
  const apply = (patch, edited) => {
    const next = mergeSettings({ ...D, ...patch }, edited);
    return [next, settingNotice(edited, patch[edited], next, D)];
  };
  assert.equal(apply({ maxGenres: 20 }, "maxGenres")[1], "");
  assert.match(apply({ maxGenres: 1000 }, "maxGenres")[1], /Genres shown must be 3 to 40; 40 is used/);
  assert.match(apply({ thinBelow: 0 }, "thinBelow")[1], /Thin limit must be 1 to 20; 1 is used/);
  assert.match(apply({ thinBelow: NaN }, "thinBelow")[1], /must be 1 to 20/);
  assert.equal(apply({ bpmMin: 70 }, "bpmMin")[1], ""); // 70..200 is whole bands of 10
  assert.match(apply({ bpmMax: 62 }, "bpmMax")[1], /BPM range is now 60 to 70/);
  assert.match(apply({ bpmMin: 65 }, "bpmMin")[1], /now 65 to 195/); // max trimmed to whole bands
  assert.equal(apply({ bandWidth: 20 }, "bandWidth")[1], "");
  assert.match(apply({ bpmMin: 5 }, "bpmMin")[1], /must be 20 to 300/);
});

test("hasBpm follows the Tracks table: needs-analysis tracks have none", () => {
  const t = track({ bpm: 120 });
  assert.equal(hasBpm(t, NONE), true);
  assert.equal(hasBpm(t, new Map([[t.track_id, "x"]])), false);
  assert.equal(hasBpm(track({ bpm: 0 }), NONE), false);
  assert.equal(hasBpm(track({ bpm: NaN }), NONE), false);
});

// A small deterministic generator, so the property below is a fixed test, not a flaky one.
function lcg(seed) {
  let x = seed;
  return () => ((x = (x * 1664525 + 1013904223) % 4294967296) / 4294967296);
}

test("preview equals action: a cell's tracks are exactly what the Tracks filter then shows", () => {
  for (const seed of [1, 2, 3]) {
    const rnd = lcg(seed);
    const genres = ["House", "Techno", "Dub", "", "Drone"];
    const keys = ["Am", "C", "F#m", "Ab Minor", "8A", "12B", "", "??"];
    const tracks = Array.from({ length: 400 }, () =>
      track({
        genre: genres[Math.floor(rnd() * genres.length)],
        // plenty of values sitting right on band edges and just under them
        bpm: [0, 59.99, 60, 79.995, 80, 119.9999, 120, 120.0001, 128, 129.99999999999997, 130, 140.5, 199.99, 200, 250][Math.floor(rnd() * 15)] + (rnd() < 0.3 ? 0.01 * Math.floor(rnd() * 5) : 0),
        key: keys[Math.floor(rnd() * keys.length)],
      }),
    );
    const needs = new Map(tracks.filter(() => rnd() < 0.1).map((t) => [t.track_id, "never analysed"]));
    for (const width of [2, 5, 10, 20]) {
      const s = mergeSettings({ ...S, bandWidth: width });
      for (const m of [buildBpmKey(tracks, needs, s, keyCode), buildBpmGenre(tracks, needs, { ...s, maxGenres: 3 })]) {
        let checked = 0;
        m.cells.forEach((row, r) =>
          row.forEach((cell, c) => {
            const f = cellFilter(m, r, c, tracks, needs);
            if (cell.length === 0) return assert.equal(f, null);
            if (!f) return assert.equal(m.rows[r].value.kind === "otherGenres" || m.cols[c].value.kind === "otherGenres", true);
            assert.deepEqual(ids(filterTracks(tracks, f, needs)), [...cell].sort((a, b) => a - b), `${m.id} ${m.rows[r].label} x ${m.cols[c].label} width ${width}`);
            checked++;
          }),
        );
        assert.ok(checked > 5, "the property must really run");
      }
    }
  }
});

test("cellFilter refuses what the Tracks view cannot express", () => {
  const tracks = [track({ energy: 5, key: "Am" })];
  const m = buildKeyEnergy(tracks, keyCode);
  assert.equal(cellFilter(m, KEY_ORDER.indexOf("8A"), 4, tracks, NONE), null);
  const b = buildBpmKey(tracks, NONE, S, keyCode);
  assert.equal(cellFilter(b, 0, 0, tracks, NONE), null); // empty cell
  assert.equal(cellFilter(b, 99, 0, tracks, NONE), null); // no such row
});
