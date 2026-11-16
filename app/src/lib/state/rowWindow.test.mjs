// Run: node --experimental-strip-types --test src/lib/state/rowWindow.test.mjs
import assert from "node:assert/strict";
import { test } from "node:test";
import { revealTop, rowWindow } from "./rowWindow.ts";

test("an empty or unmeasured list renders nothing", () => {
  assert.deepEqual(rowWindow(0, 600, 28, 30, 0, 8), { start: 0, end: 0 });
  assert.deepEqual(rowWindow(0, 600, 0, 30, 100, 8), { start: 0, end: 0 });
});

test("at the top: the visible rows plus overscan below", () => {
  const w = rowWindow(0, 600, 28, 30, 5000, 8);
  assert.equal(w.start, 0);
  assert.equal(w.end, Math.ceil((600 - 30) / 28) + 8);
});

test("scrolled: overscan on both sides, never past the ends", () => {
  const w = rowWindow(28 * 1000 + 30, 600, 28, 30, 5000, 8);
  assert.equal(w.start, 1000 - 8);
  assert.ok(w.end >= 1000 + 20 && w.end <= 1000 + 22 + 8);
  assert.deepEqual(rowWindow(1e9, 600, 28, 30, 50, 8), { start: 41, end: 50 });
  assert.ok(rowWindow(1e9, 600, 28, 30, 50, 8).start <= 50);
});

test("every row that is on screen is inside the window, for any scroll position", () => {
  const rowH = 27.3;
  const head = 31;
  const view = 540;
  const total = 777;
  for (let st = 0; st < head + total * rowH; st += 13.7) {
    const { start, end } = rowWindow(st, view, rowH, head, total, 4);
    for (let i = 0; i < total; i++) {
      const top = head + i * rowH;
      const onScreen = top + rowH > st + head && top < st + view;
      if (onScreen) assert.ok(i >= start && i < end, `row ${i} at scroll ${st}`);
    }
  }
});

test("revealTop leaves a visible row alone and moves a hidden one the least", () => {
  assert.equal(revealTop(3, 0, 600, 28, 30), 0);
  // below the view: its bottom edge lands on the view's bottom
  assert.equal(revealTop(100, 0, 600, 28, 30), 30 + 101 * 28 - 600);
  // above the view (behind the sticky header): its top edge lands just under the header
  assert.equal(revealTop(10, 5000, 600, 28, 30), 10 * 28);
  assert.equal(revealTop(0, 5000, 600, 28, 30), 0);
});
