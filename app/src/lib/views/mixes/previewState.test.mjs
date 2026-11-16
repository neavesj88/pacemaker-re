// Tests for the rule that decides whether a rendered preview is still current (previewState.ts).
//
//   node --experimental-strip-types src/lib/views/mixes/previewState.test.mjs      (from app/)
import assert from "node:assert/strict";
import { addKept, addTracksToast, beatmatchToast, keptNote, matchAllToast, sameAudio, sameRender, volumeToDb } from "./previewState.ts";
import { wordCount, TOAST_HARD } from "../../state/toastText.ts";

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

test("tracks kept at their own tempo collect in one note", () => {
  const a = addKept([], ["A (+9 % to reach 128.0 BPM)"]);
  const b = addKept(a, ["B (-8 %)", "A (+9 % to reach 128.0 BPM)"]);
  assert.deepEqual(b, ["A (+9 % to reach 128.0 BPM)", "B (-8 %)"]);
  assert.match(keptNote(b), /A \(\+9 % to reach 128\.0 BPM\); B \(-8 %\)\./);
  assert.deepEqual(a, ["A (+9 % to reach 128.0 BPM)"], "the earlier list is not changed");
});

test("the mix editor's toasts are one short line, the long form is in the details", () => {
  const all = [];
  all.push(addTracksToast({ added: 1, moved: [], kept: [] }));
  all.push(addTracksToast({ added: 3, moved: ["A at 2:10", "B at 3:05"], kept: ["A (+9 % to reach 128.0 BPM)", "B (-8 %)"] }));
  all.push(addTracksToast({ added: 1, moved: ["A at 2:10"], kept: ["A (+9 %)"] }));
  all.push(beatmatchToast(128, []));
  all.push(beatmatchToast(126.04, ["the reference clip plays at 126.00 BPM, not the mix tempo 128.00; matched to the clip", "no first downbeat in one of the grids"]));
  all.push(matchAllToast({ applied: 5, already: 2, kept: [], ramps: [] }, 128));
  all.push(matchAllToast({ applied: 5, already: 2, kept: ["A (+25 %)", "B (-30 %)"], ramps: ["C"] }, 128.04));
  all.push(matchAllToast({ applied: 0, already: 4, kept: [], ramps: [] }, 128));
  all.push(matchAllToast({ applied: 0, already: 0, kept: ["A (+25 %)"], ramps: [] }, 128));
  for (const t of all) assert.ok(wordCount(t.text) <= TOAST_HARD, t.text);
  assert.equal(addTracksToast({ added: 1, moved: [], kept: [] }).text, "Track added");
  assert.equal(addTracksToast({ added: 3, moved: ["x"], kept: ["y"] }).text, "3 tracks added; 1 placed later, decks busy; 1 track kept at own tempo.");
  assert.match(addTracksToast({ added: 3, moved: ["A at 2:10"], kept: [] }).details, /A at 2:10/);
  assert.equal(beatmatchToast(128, []).text, "Matched to 128.0 BPM.");
  assert.equal(beatmatchToast(126, ["a", "b"]).details, "A.\nB.");
  assert.equal(matchAllToast({ applied: 5, already: 0, kept: [], ramps: [] }, 128).text, "Matched 5 tracks to 128 BPM.");
  const m = matchAllToast({ applied: 5, already: 0, kept: ["A (+25 %)"], ramps: ["C"] }, 128);
  assert.equal(m.text, "Matched 5 tracks to 128 BPM; 2 kept.");
  assert.equal(m.kind, "info");
  assert.match(m.details, /tempo ramp[^\n]*C\.\nLeft at their own tempo[^\n]*A \(\+25 %\)/);
});

console.log(`${n} passed`);
