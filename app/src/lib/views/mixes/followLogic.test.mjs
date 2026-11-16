// Tests for followLogic.ts. Run: node --experimental-strip-types --test src/lib/views/mixes/followLogic.test.mjs
import assert from "node:assert/strict";
import { test } from "node:test";
import { DEFAULT_FOLLOW_PREFS, followScroll, mergeFollowPrefs } from "./followLogic.ts";

const smooth = { mode: "smooth", anchorPct: 40 };
const page = { mode: "page", anchorPct: 40 };
// a 1000 px view, content scrollable to 5000, the following stops at 4000
const base = { view: 1000, maxScroll: 5000, stop: 4000 };

test("smooth: nothing moves until the playhead reaches the anchor", () => {
  assert.equal(followScroll({ ...base, playhead: 100, scroll: 0, prefs: smooth }), null);
  assert.equal(followScroll({ ...base, playhead: 400, scroll: 0, prefs: smooth }), null);
});

test("smooth: past the anchor the view moves with the playhead, a pixel at a time", () => {
  let scroll = 0;
  let worst = 0;
  for (let p = 380; p < 3000; p += 1.5) {
    const to = followScroll({ ...base, playhead: p, scroll, prefs: smooth });
    if (to !== null) {
      worst = Math.max(worst, Math.abs(to - scroll));
      scroll = to;
    }
    if (p > 410) assert.ok(Math.abs(p - scroll - 400) <= 1, `held at the anchor: ${p} ${scroll}`);
  }
  assert.ok(worst <= 3, `no page jump: ${worst}`);
});

test("smooth: never scrolls back while the playhead is on screen left of the anchor", () => {
  assert.equal(followScroll({ ...base, playhead: 1200, scroll: 1000, prefs: smooth }), null);
});

test("smooth: stops where the end of the content meets the right edge, then the playhead runs on", () => {
  assert.equal(followScroll({ ...base, playhead: 4800, scroll: 3990, prefs: smooth }), 4000);
  assert.equal(followScroll({ ...base, playhead: 4900, scroll: 4000, prefs: smooth }), null);
  // a short mix that fits the view never scrolls
  assert.equal(followScroll({ view: 1000, maxScroll: 0, stop: 0, playhead: 900, scroll: 0, prefs: smooth }), null);
});

test("smooth: a playhead off screen (a seek) is brought to the anchor in one step, either way", () => {
  assert.equal(followScroll({ ...base, playhead: 3400, scroll: 0, prefs: smooth }), 3000);
  assert.equal(followScroll({ ...base, playhead: 200, scroll: 3000, prefs: smooth }), 0);
  assert.equal(followScroll({ ...base, playhead: 4900, scroll: 0, prefs: smooth }), 4500);
});

test("page: the view jumps only when the playhead reaches the right edge, with no window it is lost in", () => {
  assert.equal(followScroll({ ...base, playhead: 900, scroll: 0, prefs: page }), null);
  assert.equal(followScroll({ ...base, playhead: 980, scroll: 0, prefs: page }), 830);
  // far past the edge (a seek or a slow frame) is followed too
  assert.equal(followScroll({ ...base, playhead: 2500, scroll: 0, prefs: page }), 2350);
});

test("a view with no width, or a playhead that is not a number, is left alone", () => {
  assert.equal(followScroll({ ...base, view: 0, playhead: 500, scroll: 0, prefs: smooth }), null);
  assert.equal(followScroll({ ...base, playhead: NaN, scroll: 0, prefs: smooth }), null);
});

test("saved prefs: defaults, junk ignored, anchor held inside its range", () => {
  assert.deepEqual(mergeFollowPrefs(null), DEFAULT_FOLLOW_PREFS);
  assert.deepEqual(mergeFollowPrefs("x"), DEFAULT_FOLLOW_PREFS);
  assert.deepEqual(mergeFollowPrefs({ mode: "page", anchorPct: 55.4 }), { mode: "page", anchorPct: 55 });
  assert.deepEqual(mergeFollowPrefs({ mode: "wild", anchorPct: 5 }), { mode: "smooth", anchorPct: 20 });
  assert.deepEqual(mergeFollowPrefs({ anchorPct: 500 }), { mode: "smooth", anchorPct: 80 });
  assert.equal(DEFAULT_FOLLOW_PREFS.mode, "smooth");
});
