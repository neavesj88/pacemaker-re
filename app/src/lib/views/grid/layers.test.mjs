// Run: node --experimental-strip-types --test src/lib/views/grid/layers.test.mjs
import assert from "node:assert/strict";
import { test } from "node:test";
import { bitmapSize, fitBitmap } from "./layers.ts";

test("bitmap size rounds the CSS size times the pixel ratio", () => {
  assert.deepEqual(bitmapSize(1000, 280, 1), [1000, 280]);
  assert.deepEqual(bitmapSize(1001, 280, 1.475), [1476, 413]);
});

test("a canvas is only resized when its size really changes", () => {
  let sets = 0;
  const c = {
    _w: 0,
    _h: 0,
    get width() {
      return this._w;
    },
    set width(v) {
      sets++;
      this._w = v;
    },
    get height() {
      return this._h;
    },
    set height(v) {
      sets++;
      this._h = v;
    },
  };
  assert.equal(fitBitmap(c, 100, 50), true);
  assert.equal(sets, 2);
  assert.equal(fitBitmap(c, 100, 50), false);
  assert.equal(sets, 2, "same size must not touch the canvas (that would clear it)");
  assert.equal(fitBitmap(c, 100, 60), true);
});
