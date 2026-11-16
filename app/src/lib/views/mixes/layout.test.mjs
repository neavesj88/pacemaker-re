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
  // a clip snapped onto a bar line is stored rounded to a whole sample, half a sample short
  assert.deepEqual(L.barBeat(Math.round(28.125 * L.SR), 128), { bar: 16, beat: 1 });
  assert.deepEqual(L.barBeat(Math.round(28.125 * L.SR) - 100, 128), { bar: 15, beat: 4 });
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

test("bar numbers never sit closer than their own width, even four-digit ones at the far zoom-out", () => {
  // the walkthrough read "257 513 769 1025": labels 256 bars apart still touched at the smallest zoom
  for (const pps of [0.05, 0.08, 0.2, 1, 5, 20]) {
    for (const bpm of [90, 128, 174]) {
      const to = 2000 * ((60 / bpm) * 4); // 2000 bars on screen
      const labelled = L.rulerTicks(0, to, pps, bpm).bars.filter((b) => b.label);
      for (let i = 1; i < labelled.length; i++) {
        const gap = (labelled[i].sec - labelled[i - 1].sec) * pps;
        assert.ok(gap >= L.barLabelPx(labelled[i].bar) - 1e-6, `pps ${pps}, ${bpm} BPM: ${labelled[i - 1].bar} -> ${labelled[i].bar} is ${gap.toFixed(1)} px`);
      }
    }
  }
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

test("lanes only scroll as far as the content reaches", () => {
  const end = L.SR * 100; // 100 s of content
  assert.equal(L.maxScroll(end, 5, 1132), 0); // 500 px fits the 1000 px lanes
  assert.equal(L.maxScroll(end, 20, 1132), 1000); // 2000 px in 1000 px lanes
  assert.equal(L.laneContentW(end, 5, 1132), 1000);
  assert.equal(L.maxScroll(end, 20, 50), 2000); // a window narrower than the headers
});

test("zooming keeps the anchor in place and never scrolls past the end", () => {
  const end = L.SR * 100;
  // zoom in 4x around the middle of 1000 px lanes: the time under the anchor stays under it
  const sl = L.zoomScroll(0, 5, 20, 500, end, 1132);
  assert.equal(sl, 1500 - 500);
  // zoomed right out, the lanes cannot scroll at all: the old position is not kept
  assert.equal(L.zoomScroll(1000, 20, 5, 500, end, 1132), 0);
  // the case that blanked a clip: zooming in a little at the far right while everything still fits
  // used to add a scroll position the element refused, again and again
  let pps = 5;
  let at = 0;
  for (let i = 0; i < 40; i++) {
    const next = pps * 1.09;
    at = L.zoomScroll(at, pps, next, 900, end, 1132);
    pps = next;
    assert.ok(at >= 0 && at <= L.maxScroll(end, pps, 1132) + 1e-9);
  }
});

test("the waveform window covers what is on screen at any scroll and zoom", () => {
  const STEP = 256;
  let seed = 7;
  const rnd = () => (seed = (seed * 1664525 + 1013904223) % 4294967296) / 4294967296;
  for (let i = 0; i < 5000; i++) {
    const viewW = 400 + Math.floor(rnd() * 5000);
    const lane = viewW - L.HEADER_W;
    const clipW = 2 + rnd() * 40000;
    const clipLeft = rnd() * 40000 - 5000;
    const scroll = rnd() * 45000;
    const w = L.waveWindow(scroll, viewW, clipLeft, clipW, STEP);
    assert.ok(w.from >= 0 && w.to >= w.from + 1 && w.from % STEP === 0);
    // the part of the clip inside the lanes' viewport [scroll, scroll + lane] must lie inside the window
    const vis0 = Math.max(0, scroll - clipLeft);
    const vis1 = Math.min(clipW, scroll + lane - clipLeft);
    if (vis1 > vis0) {
      assert.ok(w.from <= vis0 + 1e-9, `left gap ${JSON.stringify({ viewW, clipW, clipLeft, scroll, w })}`);
      assert.ok(w.to >= vis1 - 1e-9, `right gap ${JSON.stringify({ viewW, clipW, clipLeft, scroll, w })}`);
    }
  }
});

test("the clip bar height: full zoomed in, short far out, eased and monotonic in between", () => {
  const full = 300;
  assert.equal(L.clipBarHeight(L.SHRINK_NEAR_PPS, full), full);
  assert.equal(L.clipBarHeight(L.PPS_MAX, full), full);
  assert.equal(L.clipBarHeight(L.SHRINK_FAR_PPS, full), L.BAR_FAR);
  assert.equal(L.clipBarHeight(L.PPS_MIN, full), L.BAR_FAR);
  // halfway in log zoom is halfway in height (the smoothstep is symmetric)
  const mid = Math.sqrt(L.SHRINK_FAR_PPS * L.SHRINK_NEAR_PPS);
  assert.ok(Math.abs(L.clipBarHeight(mid, full) - (L.BAR_FAR + full) / 2) <= 1);
  let prev = 0;
  for (let pps = L.PPS_MIN; pps <= L.PPS_MAX; pps *= 1.03) {
    const h = L.clipBarHeight(pps, full);
    assert.ok(h >= prev && h >= L.BAR_FAR && h <= full && Number.isInteger(h), `pps ${pps} -> ${h}`);
    // eased: one 3 % zoom step never moves the bar by more than a few pixels
    if (prev) assert.ok(h - prev <= 6, `step too big at ${pps}: ${h - prev}`);
    prev = h;
  }
});

test("the clip bar height: Always full height never changes; a small window is not made taller", () => {
  for (const pps of [L.PPS_MIN, 0.3, 2, 9, 30, 400]) assert.equal(L.clipBarHeight(pps, 200, "full"), 200);
  assert.equal(L.clipBarHeight(0.1, 50), 50);
  assert.equal(L.clipBarHeight(0.1, L.BAR_FAR), L.BAR_FAR);
  assert.equal(L.shrinkFactor(0), 0);
  assert.equal(L.shrinkFactor(NaN), 0);
});

console.log(`\n${n} layout tests passed`);
