// Run: node --experimental-strip-types --test src/lib/player/playerSize.test.mjs
import { test } from "node:test";
import assert from "node:assert/strict";
import {
  DEFAULT_HEIGHT, MIN_HEIGHT, HARD_MAX_HEIGHT, KEY_STEP, KEY_STEP_BIG,
  maxHeight, clampHeight, normaliseHeight, scaleFor, dragHeight, keyHeight,
} from "./playerSize.ts";

test("max height is 45% of the window, never below the minimum", () => {
  assert.equal(maxHeight(2160), 972);
  assert.equal(maxHeight(1080), 486);
  assert.equal(maxHeight(100), MIN_HEIGHT);
  assert.equal(maxHeight(0), MIN_HEIGHT);
  assert.equal(maxHeight(NaN), MIN_HEIGHT);
});

test("clamp holds a height between minimum and the window's maximum", () => {
  assert.equal(clampHeight(10, 1080), MIN_HEIGHT);
  assert.equal(clampHeight(300, 1080), 300);
  assert.equal(clampHeight(900, 1080), 486);
  assert.equal(clampHeight(900, 2160), 900);
  assert.equal(clampHeight(NaN, 1080), DEFAULT_HEIGHT);
  assert.equal(clampHeight(100.6, 1080), 101);
});

test("a size remembered on a big screen is shown clamped on a small one without being lost", () => {
  const stored = normaliseHeight(900);
  assert.equal(stored, 900);
  assert.equal(clampHeight(stored, 1080), 486);
  assert.equal(clampHeight(stored, 2160), 900); // back on the big screen it is intact
});

test("stored values: missing, junk and out of range", () => {
  assert.equal(normaliseHeight(null), DEFAULT_HEIGHT);
  assert.equal(normaliseHeight(undefined), DEFAULT_HEIGHT);
  assert.equal(normaliseHeight("200"), DEFAULT_HEIGHT);
  assert.equal(normaliseHeight({}), DEFAULT_HEIGHT);
  assert.equal(normaliseHeight(NaN), DEFAULT_HEIGHT);
  assert.equal(normaliseHeight(Infinity), DEFAULT_HEIGHT);
  assert.equal(normaliseHeight(-50), MIN_HEIGHT);
  assert.equal(normaliseHeight(5), MIN_HEIGHT);
  assert.equal(normaliseHeight(99999), HARD_MAX_HEIGHT);
  assert.equal(normaliseHeight(250.4), 250);
});

test("scale is 1 at the default and never below 1", () => {
  assert.equal(scaleFor(DEFAULT_HEIGHT, 5120), 1);
  assert.equal(scaleFor(10, 5120), 1);
  assert.equal(scaleFor(DEFAULT_HEIGHT, 400), 1);
});

test("scale rises in quarter steps with height", () => {
  const seen = new Set();
  let prev = 1;
  for (let h = DEFAULT_HEIGHT; h <= 972; h++) {
    const s = scaleFor(h, 5120);
    assert.ok(s >= prev, `non-decreasing at ${h}`);
    assert.equal((s * 4) % 1, 0, `quarter step at ${h}`);
    prev = s;
    seen.add(s);
  }
  assert.ok(seen.size >= 8);
  assert.equal(scaleFor(DEFAULT_HEIGHT * 2, 5120), 1.5);
  assert.equal(scaleFor(972, 5120), 4);
});

test("scale is held back on a narrow window so the waveform keeps its width", () => {
  assert.equal(scaleFor(486, 1920), 1.5);
  assert.ok(scaleFor(486, 5120) > scaleFor(486, 1920));
  assert.equal(scaleFor(486, 1280), 1);
});

test("dragging up grows, down shrinks, and the result is clamped", () => {
  assert.equal(dragHeight(100, 500, 450, 2160), 150);
  assert.equal(dragHeight(100, 500, 540, 2160), MIN_HEIGHT);
  assert.equal(dragHeight(100, 500, -5000, 1080), 486);
  assert.equal(dragHeight(100, 500, 500, 1080), 100);
});

test("keyboard: arrows step, Shift steps big, Home and End jump, others ignored", () => {
  assert.equal(keyHeight("ArrowUp", false, 100, 1080), 100 + KEY_STEP);
  assert.equal(keyHeight("ArrowDown", false, 100, 1080), 100 - KEY_STEP);
  assert.equal(keyHeight("ArrowUp", true, 100, 1080), 100 + KEY_STEP_BIG);
  assert.equal(keyHeight("ArrowDown", false, MIN_HEIGHT, 1080), MIN_HEIGHT);
  assert.equal(keyHeight("ArrowUp", true, 480, 1080), 486);
  assert.equal(keyHeight("Home", false, 300, 1080), MIN_HEIGHT);
  assert.equal(keyHeight("End", false, 300, 1080), 486);
  assert.equal(keyHeight("End", false, 300, 2160), 972);
  assert.equal(keyHeight("a", false, 300, 1080), null);
  assert.equal(keyHeight("ArrowLeft", false, 300, 1080), null);
});
