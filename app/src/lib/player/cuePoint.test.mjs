// Run: node --experimental-strip-types --test src/lib/player/cuePoint.test.mjs
import assert from "node:assert/strict";
import { test } from "node:test";
import { waveSecs } from "./cuePoint.ts";

test("player time: fraction of the width, clamped, offset by where the file starts", () => {
  assert.equal(waveSecs(150, 100, 200, 0, 240), 60);
  assert.equal(waveSecs(50, 100, 200, 0, 240), 0);
  assert.equal(waveSecs(900, 100, 200, 0, 240), 240);
  assert.equal(waveSecs(150, 100, 200, 30, 240), 90);
  assert.equal(waveSecs(150, 100, 0, 0, 240), null);
  assert.equal(waveSecs(150, 100, 200, 0, 0), null);
});
