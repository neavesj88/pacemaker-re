// Run: node --experimental-strip-types --test src/lib/views/grid/gridDiff.test.mjs
import assert from "node:assert/strict";
import { test } from "node:test";
import { diffGrids, diffHeadline, gridBpm } from "./gridDiff.ts";

const SR = 44100;

function grid(bpm, first, n, fd = 0) {
  const period = (SR * 60) / bpm;
  return { normalize_factor: 1, beat_start_offset: first, beats: Array.from({ length: n }, (_, i) => ({ pos: first + Math.round(i * period), bpm })), first_downbeat: fd };
}

test("a grid compared with itself changes nothing", () => {
  const g = grid(126, 3000, 100);
  const s = diffGrids(g, g, SR);
  assert.deepEqual([s.same, s.moved, s.added, s.removed], [100, 0, 0, 0]);
  assert.equal(s.maxMoveMs, 0);
  assert.equal(s.uniformShiftMs, 0);
});

test("a nudge is a uniform shift of every beat line", () => {
  const a = grid(126, 3000, 100);
  const b = { ...a, beats: a.beats.map((x) => ({ ...x, pos: x.pos + 882 })) };
  const s = diffGrids(a, b, SR);
  assert.equal(s.uniformShiftMs, 20);
  assert.equal(s.moved, 100);
  assert.ok(Math.abs(s.meanMoveMs - 20) < 1e-9 && Math.abs(s.maxMoveMs - 20) < 1e-9);
  assert.equal(diffHeadline(s), "Every beat line is 20.0 ms later than the old beat line it replaces.");
  assert.ok(Math.abs(s.firstAfter - s.firstBefore - 0.02) < 1e-9);
});

test("a nudge that drops the first beat lines is still one uniform shift", () => {
  const a = grid(126, 3000, 100);
  // moved 200 ms earlier: the first beat line falls before 0 and is dropped
  const moved = a.beats.map((x) => ({ ...x, pos: x.pos - 8820 })).filter((x) => x.pos >= 0);
  assert.equal(moved.length, 99);
  const s = diffGrids(a, { ...a, beats: moved }, SR);
  assert.equal(s.uniformShiftMs, -200);
  assert.match(diffHeadline(s), /^Every beat line is 200.0 ms earlier/);
});

test("a regular grid looks the same after whole beats, so the shift reported is the nearest one", () => {
  const a = grid(126, 3000, 100);
  // 1000 ms earlier is 2.1 beats: indistinguishable from 0.1 beat (47.6 ms) earlier once two lines are dropped
  const moved = a.beats.map((x) => ({ ...x, pos: x.pos - 44100 })).filter((x) => x.pos >= 0);
  const s = diffGrids(a, { ...a, beats: moved }, SR);
  assert.ok(Math.abs(s.uniformShiftMs + 47.619) < 0.01, `${s.uniformShiftMs}`);
});

test("halving keeps every second beat line and drops the rest", () => {
  const a = grid(172, 3000, 200);
  const b = { ...a, beats: a.beats.filter((_, i) => i % 2 === 0).map((x) => ({ pos: x.pos, bpm: 86 })) };
  const s = diffGrids(a, b, SR);
  assert.equal(s.same, 100);
  assert.equal(s.removed, 100);
  assert.equal(s.added, 0);
  assert.equal(s.bpmBefore, 172);
  assert.equal(s.bpmAfter, 86);
  assert.equal(diffHeadline(s), "Of 200 old beat lines, 100 stay, 100 dropped.");
});

test("doubling keeps the old lines and adds one between each pair", () => {
  const a = grid(86, 3000, 100);
  const b = grid(172, 3000, 200);
  const s = diffGrids(a, b, SR);
  assert.equal(s.removed, 0);
  assert.ok(s.added >= 99 && s.added <= 101, `${s.added}`);
  assert.ok(s.same + s.moved >= 99);
  assert.match(diffHeadline(s), /added/);
});

test("a re-analysis with a different tempo and phase matches little and says so", () => {
  const s = diffGrids(grid(120, 3000, 100), grid(128, 9000, 100), SR);
  assert.equal(s.uniformShiftMs, null);
  assert.ok(s.added > 0 && s.removed > 0);
  assert.equal(s.bpmBefore, 120);
  assert.equal(s.bpmAfter, 128);
});

test("the first bar line is reported for both grids, and a missing old grid is handled", () => {
  const a = grid(126, 3000, 40, 1);
  const b = grid(126, 3000, 40, 3);
  const s = diffGrids(a, b, SR);
  assert.ok(Math.abs(s.barBefore - a.beats[1].pos / SR) < 1e-12);
  assert.ok(Math.abs(s.barAfter - a.beats[3].pos / SR) < 1e-12);
  const none = diffGrids(null, b, SR);
  assert.equal(none.beatsBefore, 0);
  assert.equal(none.bpmBefore, null);
  assert.equal(none.added, 40);
  assert.equal(diffHeadline(none), "There was no grid before; the new one has 40 beat lines.");
  assert.equal(gridBpm(b), 126);
});
