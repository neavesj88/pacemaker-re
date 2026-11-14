// Run: node --experimental-strip-types --test src/lib/views/grid/gridMath.test.mjs
import assert from "node:assert/strict";
import { test } from "node:test";
import { firstDownbeatSecs } from "./gridMath.ts";

const beats = [0.4, 0.9, 1.4, 1.9, 2.4].map((s) => ({ pos: Math.round(s * 1000) }));

test("the first downbeat is the beat the grid marks, not the first beat", () => {
  assert.equal(firstDownbeatSecs({ beats, first_downbeat: 0 }, 1000), 0.4);
  assert.equal(firstDownbeatSecs({ beats, first_downbeat: 2 }, 1000), 1.4);
});

test("Set downbeat moves it: a new index gives a new time", () => {
  const before = firstDownbeatSecs({ beats, first_downbeat: 1 }, 1000);
  const after = firstDownbeatSecs({ beats, first_downbeat: 3 }, 1000);
  assert.notEqual(before, after);
});

test("no downbeat marked, an index past the beats, no grid or a bad rate give null", () => {
  assert.equal(firstDownbeatSecs({ beats, first_downbeat: null }, 1000), null);
  assert.equal(firstDownbeatSecs({ beats, first_downbeat: 9 }, 1000), null);
  assert.equal(firstDownbeatSecs(null, 1000), null);
  assert.equal(firstDownbeatSecs({ beats, first_downbeat: 0 }, 0), null);
});
