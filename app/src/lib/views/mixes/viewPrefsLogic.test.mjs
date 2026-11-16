// Tests for viewPrefsLogic.ts. Run: node --experimental-strip-types --test src/lib/views/mixes/viewPrefsLogic.test.mjs
import assert from "node:assert/strict";
import { test } from "node:test";
import { DEFAULT_VIEW_PREFS, mergeViewPrefs } from "./viewPrefsLogic.ts";

test("the defaults: clips shrink as you zoom out, the whole-mix strip is on, following is on", () => {
  assert.deepEqual(DEFAULT_VIEW_PREFS, { clipHeight: "shrink", overview: true, follow: true });
  assert.deepEqual(mergeViewPrefs(null), DEFAULT_VIEW_PREFS);
  assert.deepEqual(mergeViewPrefs("junk"), DEFAULT_VIEW_PREFS);
});

test("saved choices are kept, anything unknown or malformed is ignored", () => {
  assert.deepEqual(mergeViewPrefs({ clipHeight: "full", overview: false, follow: false }), { clipHeight: "full", overview: false, follow: false });
  assert.deepEqual(mergeViewPrefs({ clipHeight: "tall", overview: "no", follow: 0, extra: 1 }), DEFAULT_VIEW_PREFS);
  assert.deepEqual(mergeViewPrefs({ overview: false }), { ...DEFAULT_VIEW_PREFS, overview: false });
});
