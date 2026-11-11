// Tests for the pure layout helpers of the mix timeline (layout.ts). Plain node script, like
// scripts/test-mixedit.mjs (no test runner is installed):
//
//   node --experimental-strip-types src/lib/views/mixes/layout.test.mjs      (from app/)
//
// Exits non-zero on the first failed assertion.
import assert from "node:assert/strict";
import * as L from "./layout.ts";

let n = 0;
const test = (name, fn) => {
  fn();
  n++;
  console.log(`ok  ${name}`);
};

test("zoom slider is a log scale that round-trips", () => {
  assert.equal(L.ppsFromSlider(0), L.PPS_MIN);
  assert.ok(Math.abs(L.ppsFromSlider(100) - L.PPS_MAX) < 1e-9);
  for (const v of [0, 13, 50, 77, 100]) assert.ok(Math.abs(L.sliderFromPps(L.ppsFromSlider(v)) - v) < 1e-9);
  assert.equal(L.sliderFromPps(10000), 100);
});

test("pixels and samples convert both ways", () => {
  assert.equal(L.toPx(L.SR * 2, 50), 100);
  assert.equal(L.fromPx(100, 50), L.SR * 2);
  assert.equal(L.toSamples(1.5), 66150);
});

test("clock and mmss text", () => {
  assert.equal(L.clock(0), "0:00.0");
  assert.equal(L.clock(75.46), "1:15.4");
  assert.equal(L.clock(-3), "0:00.0");
  assert.equal(L.mmss(599.6), "10:00");
});

test("bar and beat at the mix tempo", () => {
  // 120 BPM: a beat is 0.5 s, a bar 2 s
  assert.deepEqual(L.barBeat(0, 120), { bar: 1, beat: 1 });
  assert.deepEqual(L.barBeat(L.SR * 2.5, 120), { bar: 2, beat: 2 });
  assert.deepEqual(L.barBeat(L.SR * 7.99, 120), { bar: 4, beat: 4 });
});

test("ruler ticks: grid-aligned, labelled sparsely, beats only when roomy", () => {
  const far = L.rulerTicks(0, 600, 1, 120);
  assert.ok(far.time.length > 3 && far.time.length < 20);
  assert.ok(far.time.every((t) => t.sec * 1 >= 0));
  assert.equal(far.beats.length, 0); // 0.5 px per beat: too dense
  const near = L.rulerTicks(10, 20, 100, 120);
  assert.ok(near.beats.length > 0);
  assert.ok(near.bars.every((b) => Math.abs(b.sec / 2 - Math.round(b.sec / 2)) < 1e-9));
  assert.ok(near.bars.some((b) => b.label));
  // scrolling does not move marks: the same bar appears at the same time in two windows
  const a = L.rulerTicks(0, 30, 60, 128).bars.find((b) => b.bar === 5);
  const b = L.rulerTicks(5, 40, 60, 128).bars.find((x) => x.bar === 5);
  assert.equal(a?.sec, b?.sec);
  assert.deepEqual(L.rulerTicks(0, 10, 50, 0).bars, []);
});

test("packRows puts overlapping ranges on different rows", () => {
  const items = [
    { start: 0, end: 10 },
    { start: 5, end: 15 },
    { start: 10, end: 20 },
    { start: 12, end: 14 },
  ];
  const { rows, count } = L.packRows(items);
  assert.equal(rows[0], rows[2]); // touching ranges share a row
  assert.notEqual(rows[0], rows[1]);
  assert.notEqual(rows[1], rows[3]);
  assert.equal(count, 2 + (rows[3] > 1 ? 1 : 0));
  assert.deepEqual(L.packRows([]), { rows: [], count: 1 });
});

test("overlapRange", () => {
  assert.deepEqual(L.overlapRange(0, 10, 5, 20), [5, 10]);
  assert.equal(L.overlapRange(0, 10, 10, 20), null);
});

test("curve presets, naming and lookup", () => {
  for (const p of L.CURVE_PRESETS) assert.equal(L.curveName(p.curve), p.name);
  assert.equal(L.curveName([{ x: 0, value: 0 }, { x: 0.3, value: 0.9 }, { x: 1, value: 1 }]), "Custom");
  const lin = L.CURVE_PRESETS[0].curve;
  assert.equal(L.curveAt(lin, 0.25), 0.25);
  assert.equal(L.curveAt(lin, -1), 0);
  assert.equal(L.curveAt(lin, 2), 1);
  for (const p of L.CURVE_PRESETS) {
    const xs = p.curve.map((c) => c.x);
    assert.deepEqual(xs, [...xs].sort((a, b) => a - b));
  }
});

test("curve and key adapters round-trip", () => {
  const curve = [{ x: 0, value: 0 }, { x: 0.25, value: 0.8 }, { x: 1, value: 1 }];
  assert.deepEqual(L.envToCurve(L.curveToEnv(curve)), curve);
  const key = [{ pos: 0, cents: 0, flags: 1 }, { pos: 500, cents: -300, flags: 1 }];
  const env = L.keyToEnv(key);
  assert.deepEqual(env, [{ pos: 0, value: 0 }, { pos: 500, value: -3 }]);
  assert.deepEqual(L.envToKey(env, key), key);
  // a new node gets flags 1, an existing one keeps its own
  const withNew = L.envToKey([...env, { pos: 900, value: 2 }], [{ pos: 0, cents: 0, flags: 7 }, key[1]]);
  assert.equal(withNew[0].flags, 7);
  assert.equal(withNew[2].flags, 1);
  assert.equal(withNew[2].cents, 200);
});

test("small formatters", () => {
  assert.equal(L.roundTo(2.37, 0.5), 2.5);
  assert.equal(L.roundTo(2.37, 0), 2.37);
  assert.equal(L.fmtValue(3.14159, "dB"), "3.14 dB");
  assert.equal(L.fmtValue(1500, "ms"), "1500 ms");
  assert.equal(L.fmtValue(25), "25");
  assert.equal(L.fmtSemis(2), "+2 st");
  assert.equal(L.fmtSemis(-1), "−1 st");
  assert.equal(L.fmtSemis(0), "0 st");
  assert.equal(L.peakColumns(1), 512);
  assert.equal(L.peakColumns(200), 4000);
  assert.equal(L.peakColumns(100000), 8192);
});

test("tool colours and labels fall back sensibly", () => {
  assert.equal(L.toolLabel("FILTER"), "Filter");
  assert.equal(L.toolLabel("Reverb"), "Reverb");
  assert.ok(L.toolColour("EQ").startsWith("var("));
  assert.ok(L.toolColour("nonsense").startsWith("var("));
});

console.log(`\n${n} layout tests passed`);
