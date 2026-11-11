// Run: node --experimental-strip-types --test src/lib/views/import/importLogic.test.mjs
import assert from "node:assert/strict";
import { test } from "node:test";
import { cueApplies, cueSecs, cueTime } from "./importLogic.ts";

test("cue modes write exactly what the backend's CueMode::applies writes", () => {
  const table = {
    off: { set: false, replace: false, same: false },
    empty: { set: true, replace: false, same: false },
    overwrite: { set: true, replace: true, same: false },
  };
  for (const [mode, row] of Object.entries(table))
    for (const [action, want] of Object.entries(row)) assert.equal(cueApplies(mode, action), want, `${mode}/${action}`);
});

test("cue positions are shown to a tenth of a second", () => {
  assert.equal(cueTime(0), "0:00.0");
  assert.equal(cueTime(12.25), "0:12.2");
  assert.equal(cueTime(92.5), "1:32.5");
  assert.equal(cueTime(3599.99), "59:59.9");
  assert.equal(cueTime(-4), "0:00.0");
  assert.equal(cueTime(Number.NaN), "0:00.0");
});

test("an existing cue is converted with the new cue's own ratio", () => {
  const c = { cue_frames: 66150, cue_secs: 1.5 };
  assert.equal(cueSecs(c, 44100), 1);
  assert.equal(cueSecs({ cue_frames: 0, cue_secs: 0 }, 5), 0);
});
