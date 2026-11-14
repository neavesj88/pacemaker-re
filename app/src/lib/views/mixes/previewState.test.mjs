// Tests for the rule that decides whether a rendered preview is still current (previewState.ts).
//
//   node --experimental-strip-types src/lib/views/mixes/previewState.test.mjs      (from app/)
import assert from "node:assert/strict";
import { sameAudio, sameRender, volumeToDb } from "./previewState.ts";

let n = 0;
const test = (name, fn) => {
  fn();
  n++;
  console.log(`ok  ${name}`);
};

const clips = [{ id: 1 }];
const doc = { id: 7, name: "A", meta: { x: 1 }, tempoBpm: 120, clips, lengthSec: 60 };

test("a rename, meta or reference tempo change does not change the sound", () => {
  assert.ok(sameAudio(doc, { ...doc, name: "B", meta: {}, tempoBpm: 99 }));
  assert.ok(sameAudio(null, null));
  assert.ok(!sameAudio(doc, null));
});

test("an edited part makes the audio out of date", () => {
  assert.ok(!sameAudio(doc, { ...doc, clips: [{ id: 1 }] }));
  assert.ok(!sameAudio(doc, { ...doc, lengthSec: 61 }));
});

test("a preview rendered in one Vocal mode is out of date in another", () => {
  assert.ok(sameRender(doc, "off", doc, "off"));
  assert.ok(!sameRender(doc, "off", doc, "vocal"));
  assert.ok(!sameRender(doc, "vocal", doc, "inverted"));
  assert.ok(!sameRender(doc, "inverted", doc, "off"));
  assert.ok(sameRender(doc, "vocal", { ...doc, name: "C" }, "vocal"));
  assert.ok(!sameRender(doc, "vocal", { ...doc, lengthSec: 5 }, "vocal"));
});

test("the volume slider maps to a listening level in dB", () => {
  assert.equal(volumeToDb(1), 0);
  assert.equal(volumeToDb(0), -60);
  assert.ok(Math.abs(volumeToDb(0.5) + 6.0206) < 1e-3);
  assert.equal(volumeToDb(0.0001), -60);
  assert.equal(volumeToDb(2), 0);
});

console.log(`${n} passed`);
