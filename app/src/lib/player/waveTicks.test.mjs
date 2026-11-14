import test from "node:test";
import assert from "node:assert/strict";
import { tickTier } from "./waveTicks.ts";

test("a normal track keeps every beat tick", () => assert.equal(tickTier(900 / 600), "bars"));
test("wide beats draw all ticks", () => assert.equal(tickTier(5), "all"));
test("narrow beats draw bars only", () => assert.equal(tickTier(2), "bars"));
test("an hour-long mix draws no ticks over the peaks", () => {
  // 59:59 at 87 BPM is about 5200 beats across a 900 px wave
  assert.equal(tickTier(900 / 5200), "none");
});
test("the bar tier ends at 2 px per bar by default", () => {
  assert.equal(tickTier(0.5), "bars");
  assert.equal(tickTier(0.49), "none");
});
test("a 4 minute track on a 310 px wave keeps its bar lines", () => assert.equal(tickTier(310 / 512), "bars"));
test("the cutoff is the setting's", () => {
  assert.equal(tickTier(1.2, 4), "bars");
  assert.equal(tickTier(0.9, 4), "none");
  assert.equal(tickTier(1.2, 8), "none");
  assert.equal(tickTier(2.1, 8), "bars");
  assert.equal(tickTier(5, 0), "none");
});
