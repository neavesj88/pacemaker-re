// Tests for the pure export-dialog helpers (exportprefs.ts). Plain node script, like the other
// tests of this project (`npm test`). Exits non-zero on the first failed assertion.
import assert from "node:assert/strict";
import * as P from "./exportprefs.ts";

let n = 0;
const test = (name, fn) => {
  fn();
  n++;
  console.log(`ok  ${name}`);
};

test("the defaults do nothing to the audio", () => {
  const l = P.levelsOf(P.defaultPrefs());
  assert.equal(l.targetLufs, null);
  assert.equal(l.clipLevel, "off");
  assert.equal(P.levelsActive(l), false);
  assert.equal(P.problemWith(P.defaultPrefs()), null);
  // the values the backend defaults to (LevelSettings::default in loudness.rs)
  assert.deepEqual(l, { targetLufs: null, maxBoostDb: 12, maxCutDb: 24, capAtPeak: false, peakCeilingDb: -1, clipLevel: "off", clipFixedLufs: -14, clipMaxDb: 9 });
  assert.deepEqual(P.rangePrefsOf(P.defaultPrefs()), { endAt: "clipEnd", edgeFadeMs: 20 });
});

test("the presets name the targets people quote", () => {
  const at = (targetPreset, customTarget = -14) => P.targetOf({ ...P.defaultPrefs(), targetPreset, customTarget });
  assert.equal(at("off"), null);
  assert.equal(at("streaming"), -14);
  assert.equal(at("club"), -9);
  assert.equal(at("custom", -11.5), -11.5);
  assert.equal(P.levelsActive(P.levelsOf({ ...P.defaultPrefs(), targetPreset: "club" })), true);
  assert.equal(P.levelsActive(P.levelsOf({ ...P.defaultPrefs(), clipLevel: "median" })), true);
});

test("settings read back from storage fall back to the default for anything wrong", () => {
  assert.deepEqual(P.mergePrefs(null), P.defaultPrefs());
  assert.deepEqual(P.mergePrefs("x"), P.defaultPrefs());
  const p = P.mergePrefs({ targetPreset: "club", customTarget: -10, maxBoostDb: 99, maxCutDb: "x", clipLevel: "median", clipMaxDb: 6, endAt: "nextEnters", edgeFadeMs: 0, capAtPeak: true, previewAtExportLevel: false });
  assert.equal(p.targetPreset, "club");
  assert.equal(p.customTarget, -10);
  assert.equal(p.maxBoostDb, 12, "out of range: default");
  assert.equal(p.maxCutDb, 24, "wrong type: default");
  assert.equal(p.clipLevel, "median");
  assert.equal(p.clipMaxDb, 6);
  assert.equal(p.endAt, "nextEnters");
  assert.equal(p.edgeFadeMs, 0, "0 is a valid fade: a hard cut");
  assert.equal(p.capAtPeak, true);
  assert.equal(p.previewAtExportLevel, false);
  assert.equal(P.mergePrefs({ targetPreset: "bogus" }).targetPreset, "off");
  assert.equal(P.mergePrefs({ customTarget: NaN }).customTarget, -14);
});

test("defaults are not shared between callers", () => {
  const a = P.defaultPrefs();
  a.maxBoostDb = 3;
  assert.equal(P.defaultPrefs().maxBoostDb, 12);
  assert.equal(P.DEFAULT_PREFS.maxBoostDb, 12);
});

test("a bad value is named before anything starts", () => {
  const bad = (patch) => P.problemWith({ ...P.defaultPrefs(), ...patch });
  assert.match(bad({ targetPreset: "custom", customTarget: 3 }), /target loudness must be between -60 and 0/);
  assert.equal(bad({ targetPreset: "off", customTarget: 3 }), null, "an unused custom value does not matter");
  assert.match(bad({ maxBoostDb: -1 }), /maximum boost/);
  assert.match(bad({ maxCutDb: NaN }), /maximum cut/);
  assert.match(bad({ capAtPeak: true, peakCeilingDb: 2 }), /ceiling/);
  assert.equal(bad({ capAtPeak: false, peakCeilingDb: 2 }), null);
  assert.match(bad({ clipLevel: "fixed", clipFixedLufs: 5 }), /clip level/);
  assert.match(bad({ clipLevel: "median", clipMaxDb: 99 }), /maximum clip gain/);
  assert.match(bad({ edgeFadeMs: 5000 }), /fade at the cuts/);
});

test("times are read in the forms people type", () => {
  assert.equal(P.parseClock("83"), 83);
  assert.equal(P.parseClock("1:23"), 83);
  assert.equal(P.parseClock(" 1:23.5 "), 83.5);
  assert.equal(P.parseClock("1:23,5"), 83.5);
  assert.equal(P.parseClock("1:02:03"), 3723);
  assert.equal(P.parseClock("0:00"), 0);
  for (const bad of ["", "abc", "1:75", "-5", "1:", ":30", "1:2:3:4", "1.5.2", "1:60"]) assert.equal(P.parseClock(bad), null, bad);
});

test("clock text reads back as the same time", () => {
  for (const s of [0, 5.5, 59.9, 83.5, 600, 3723.4]) {
    const back = P.parseClock(P.clockText(s));
    assert.ok(Math.abs(back - s) <= 0.05 + 1e-9, `${s} -> ${P.clockText(s)} -> ${back}`);
  }
  assert.equal(P.clockText(83.5), "1:23.5");
  assert.equal(P.clockText(3723.4), "1:02:03.4");
  assert.equal(P.clockText(-3), "0:00.0");
  assert.equal(P.clockText(59.96), "1:00.0", "rounds up into the next minute instead of printing 0:59.10");
  assert.equal(P.durationText(75.4), "1:15");
  assert.equal(P.durationText(3600), "1:00:00");
});

test("the range request is what the backend reads", () => {
  assert.deepEqual(P.rangeSpecOf("whole", null, null, "", ""), { kind: "whole" });
  assert.deepEqual(P.rangeSpecOf("time", 10, 20, "", ""), { kind: "time", fromSec: 10, toSec: 20 });
  assert.equal(P.rangeSpecOf("time", null, 20, "", ""), null, "an unreadable time is no request");
  assert.deepEqual(P.rangeSpecOf("clips", null, null, "a", "b"), { kind: "clips", fromClip: "a", toClip: "b" });
  assert.equal(P.rangeSpecOf("clips", null, null, "a", ""), null);
});

test("an analysis answers one question: any change makes it stale", () => {
  const l = P.levelsOf({ ...P.defaultPrefs(), targetPreset: "streaming" });
  const rp = P.rangePrefsOf(P.defaultPrefs());
  const whole = { kind: "whole" };
  const key = P.analysisKey(l, whole, rp);
  assert.equal(key, P.analysisKey({ ...l }, { kind: "whole" }, { ...rp }));
  assert.notEqual(key, P.analysisKey({ ...l, targetLufs: -9 }, whole, rp));
  assert.notEqual(key, P.analysisKey({ ...l, clipLevel: "median" }, whole, rp));
  assert.notEqual(key, P.analysisKey(l, { kind: "time", fromSec: 1, toSec: 2 }, rp));
  assert.notEqual(key, P.analysisKey(l, whole, { ...rp, endAt: "nextEnters" }));
});

test("a preview plays at the export level only with a current analysis, and only if asked", () => {
  const gains = { clipGainDb: { a: -2 }, masterGainDb: 3 };
  const report = { gains };
  assert.equal(P.previewGainsOf(report, true, true), gains);
  assert.equal(P.previewGainsOf(report, false, true), undefined, "stale analysis");
  assert.equal(P.previewGainsOf(report, true, false), undefined, "the user turned it off");
  assert.equal(P.previewGainsOf(null, true, true), undefined);
  assert.equal(P.previewGainsOf({ gains: { clipGainDb: { a: 0 }, masterGainDb: 0 } }, true, true), undefined, "no gain, nothing to play differently");
});

test("readings are worded for people", () => {
  assert.equal(P.lufsText(-14.04), "-14.0 LUFS");
  assert.equal(P.lufsText(null), "none");
  assert.equal(P.peakText(-1), "-1.0 dBTP");
  assert.equal(P.peakText(undefined), "none");
  assert.equal(P.gainText(3.24), "+3.2 dB");
  assert.equal(P.gainText(-6.02), "-6.0 dB");
  assert.equal(P.gainText(0.01), "0.0 dB");
});

console.log(`${n} tests passed`);
