// Tests for overview.ts (the whole-mix strip's maths). Run: node --experimental-strip-types --test src/lib/views/mixes/overview.test.mjs
import assert from "node:assert/strict";
import { test } from "node:test";
import { BOX_MIN_W, centreScroll, dragScroll, onBox, overviewSpan, stripX, visibleBox } from "./overview.ts";

// a 3600 s mix on a 1200 px strip (3 s per px), the lanes 1000 px wide at 2 px/s (500 s on screen)
const span = 3600;
const W = 1200;

test("the box shows the part of the mix on screen, and stays inside the strip", () => {
  const b = visibleBox(0, 1000, 2, span, W);
  assert.equal(b.x0, 0);
  assert.ok(Math.abs(b.x1 - (500 / span) * W) < 1e-9);
  // scrolled 600 s in
  const c = visibleBox(1200, 1000, 2, span, W);
  assert.ok(Math.abs(c.x0 - 200) < 1e-9 && Math.abs(c.x1 - (200 + 500 / 3)) < 1e-9);
  // scrolled into the empty room past the mix end: the box stops at the strip's edge
  const d = visibleBox(20000, 1000, 2, span, W);
  assert.equal(d.x1, W);
  assert.ok(d.x1 - d.x0 >= BOX_MIN_W);
});

test("the box has a minimum width and covers the strip when everything is on screen", () => {
  const tiny = visibleBox(0, 1000, 400, span, W);
  assert.equal(tiny.x1 - tiny.x0, BOX_MIN_W);
  const all = visibleBox(0, 1000, 0.05, span, W);
  assert.deepEqual(all, { x0: 0, x1: W });
  assert.deepEqual(visibleBox(0, 1000, 2, span, 0), { x0: 0, x1: 0 });
  assert.deepEqual(visibleBox(0, 1000, 0, span, W), { x0: 0, x1: 0 });
});

test("a click centres the view on that time, within what can scroll", () => {
  const max = 6000;
  // 600 s is strip pixel 200: the lanes' middle (500 px = 250 s from their left) lands on it
  assert.equal(centreScroll(200, W, span, 2, 1000, max), 600 * 2 - 500);
  // near the start and the end it is clamped
  assert.equal(centreScroll(2, W, span, 2, 1000, max), 0);
  assert.equal(centreScroll(W, W, span, 2, 1000, 5000), 5000);
  // a click outside the strip counts as its edge
  assert.equal(centreScroll(-50, W, span, 2, 1000, max), 0);
  assert.equal(centreScroll(W + 50, W, span, 2, 1000, 5000), 5000);
  assert.equal(centreScroll(10, 0, span, 2, 1000, max), 0);
});

test("dragging the box scrolls by the same share of the mix, within what can scroll", () => {
  const max = 6000;
  assert.equal(dragScroll(1000, 100, W, span, 2, max), 1000 + 600);
  assert.equal(dragScroll(1000, -100, W, span, 2, max), 400);
  assert.equal(dragScroll(1000, -5000, W, span, 2, max), 0);
  assert.equal(dragScroll(1000, 5000, W, span, 2, max), max);
  assert.equal(dragScroll(1000, 100, 0, span, 2, max), 1000);
});

test("dragging the box and the box agree: the box follows the pointer one to one", () => {
  const max = 100000;
  const pps = 3;
  const start = 2000;
  const b0 = visibleBox(start, 1000, pps, span, W);
  for (const dx of [-120, -3, 0, 7, 250]) {
    const b = visibleBox(dragScroll(start, dx, W, span, pps, max), 1000, pps, span, W);
    assert.ok(Math.abs(b.x0 - (b0.x0 + dx)) < 1e-6, `${dx}`);
  }
});

test("the span of a short mix, the strip pixel of a time, and the hit test of the box", () => {
  assert.equal(overviewSpan(0), 10);
  assert.equal(overviewSpan(300), 300);
  assert.equal(stripX(1800, span, W), 600);
  assert.equal(stripX(99999, span, W), W);
  assert.equal(stripX(-5, span, W), 0);
  assert.equal(onBox(50, { x0: 40, x1: 60 }), true);
  assert.equal(onBox(61, { x0: 40, x1: 60 }), false);
});
