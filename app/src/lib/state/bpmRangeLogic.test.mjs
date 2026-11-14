import assert from "node:assert/strict";
import { test } from "node:test";
import { BPM_RANGES, DEFAULT_RANGE_LO, normaliseLo, offerLine, outsideRange, rangeLabel, rangeOf } from "./bpmRangeLogic.ts";

const t = (id, bpm, over = {}) => ({ track_id: id, title: `T${id}`, artist: "", location: `/pmdb_tracks/${id}.mp3`, bpm, analysed: true, ...over });

test("the default window is 88-176 and the offered windows are the backend's", () => {
  assert.equal(DEFAULT_RANGE_LO, 88);
  assert.deepEqual(BPM_RANGES.map((r) => r.lo), [98, 88, 78, 68]);
  assert.equal(rangeLabel(88), "88-176");
  assert.equal(rangeOf(78).label, "78-156");
  assert.equal(rangeOf(1).lo, 88, "an unknown value falls back to the default");
  assert.equal(normaliseLo(NaN), 88);
  assert.equal(normaliseLo("98"), 88);
  assert.equal(normaliseLo(98), 98);
});

test("tracks read at half time under 88-176 are found, in-window tracks are not", () => {
  const tracks = [t(1, 87), t(2, 87.5), t(3, 174), t(4, 128), t(5, 88), t(6, 175.9), t(7, 176), t(8, 0), t(9, 87, { analysed: false })];
  assert.deepEqual(outsideRange(tracks, 88).map((x) => x.track_id), [1, 2, 7]);
});

test("the same library under each window", () => {
  const tracks = [t(1, 87), t(2, 174), t(3, 128), t(4, 90), t(5, 180)];
  assert.deepEqual(outsideRange(tracks, 78).map((x) => x.track_id), [2, 5], "Editor window: 174 and 180 are too fast");
  assert.deepEqual(outsideRange(tracks, 98).map((x) => x.track_id), [1, 4], "98-196: 87 and 90 are too slow");
  assert.deepEqual(outsideRange(tracks, 68).map((x) => x.track_id), [2, 5], "68-136: only 174 and 180 are too fast");
});

test("tracks without a readable grid are left out", () => {
  const tracks = [t(1, 87), t(2, 87)];
  assert.deepEqual(outsideRange(tracks, 88, new Map([[2, "no .str file"]])).map((x) => x.track_id), [1]);
  assert.deepEqual(outsideRange(tracks, 88, new Set([1, 2])), []);
});

test("a track re-analysed into the window is not offered again", () => {
  const before = [t(1, 87)];
  assert.equal(outsideRange(before, 88).length, 1);
  assert.equal(outsideRange([t(1, 174)], 88).length, 0);
});

test("offer lines name the track and its BPM", () => {
  assert.equal(offerLine(t(1, 87, { artist: "A", title: "B" })), "A - B  (87.00 BPM)");
  assert.equal(offerLine(t(2, 87.5, { title: "" })), "2.mp3  (87.50 BPM)");
});

import { offerText } from "./bpmRangeLogic.ts";

test("the offer warns that slow and fast music moves, and the 88-176 hint says 176+ reads at half time", () => {
  assert.match(offerText(88), /80 BPM track would be read as 160/);
  assert.match(offerText(88), /untick/);
  assert.match(rangeOf(88).hint, /176 or faster reads at half time/);
});
