// node --experimental-strip-types src/lib/views/analysis/energyLogic.test.mjs
import assert from "node:assert/strict";
import test from "node:test";
import {
  cellTitle, clampSetting, coverageText, DEFAULT_SETTINGS, energyById, energyLabel, LIMITS, meterPercent, runTitle, sameSettings, scaleText, toAnalyse,
} from "./energyLogic.ts";

const state = (over = {}) => ({
  entries: [], total: 10, ready: 6, skipped: 1, stale: 1, none: 2, scale_used: "fit", anchor_lo: 0.31, anchor_hi: 0.72, note: null, settings: DEFAULT_SETTINGS, problem: null, ...over,
});

test("settings are clamped to the backend's ranges, whole numbers, NaN to the default", () => {
  assert.equal(clampSetting("threads", 99), LIMITS.threads.max);
  assert.equal(clampSetting("threads", 0), 1);
  assert.equal(clampSetting("threads", 2.6), 3);
  assert.equal(clampSetting("fit_low_pct", -5), 0);
  assert.equal(clampSetting("fit_low_pct", 41), 40);
  assert.equal(clampSetting("fit_high_pct", 59), 60);
  assert.equal(clampSetting("fit_high_pct", 120), 100);
  assert.equal(clampSetting("min_secs", 1), 5);
  assert.equal(clampSetting("min_secs", 9999), 600);
  assert.equal(clampSetting("min_secs", NaN), DEFAULT_SETTINGS.min_secs);
  assert.equal(clampSetting("min_secs", Infinity), DEFAULT_SETTINGS.min_secs);
});

test("the defaults sit inside their own limits and compare equal to a copy", () => {
  for (const k of Object.keys(LIMITS)) assert.equal(clampSetting(k, DEFAULT_SETTINGS[k]), DEFAULT_SETTINGS[k]);
  assert.ok(sameSettings(DEFAULT_SETTINGS, { ...DEFAULT_SETTINGS }));
  assert.ok(!sameSettings(DEFAULT_SETTINGS, { ...DEFAULT_SETTINGS, scale: "fixed" }));
  assert.ok(!sameSettings(DEFAULT_SETTINGS, { ...DEFAULT_SETTINGS, threads: 3 }));
});

test("lookup by track id and the meter", () => {
  const m = energyById([{ track_id: 4, energy: 7.4, level: 7, score: 0.6 }, { track_id: 9, energy: 1, level: 1, score: 0 }]);
  assert.equal(m.get(4).energy, 7.4);
  assert.equal(m.get(5), undefined);
  assert.equal(meterPercent(1), 10);
  assert.equal(meterPercent(10), 100);
  assert.ok(meterPercent(5) > meterPercent(4));
  assert.equal(meterPercent(0), 10, "below the scale is clamped");
  assert.equal(meterPercent(99), 100);
  assert.equal(energyLabel(7), "7.0");
  assert.equal(energyLabel(7.46), "7.5");
});

test("the cell tooltip says it is an estimate kept only by the app", () => {
  const t = cellTitle({ track_id: 1, energy: 7.4, level: 7, score: 0.612 });
  assert.match(t, /7\.4 of 10/);
  assert.match(t, /not written to the device/);
});

test("coverage wording covers each state and the singular forms", () => {
  assert.equal(coverageText(state({ total: 0, ready: 0, skipped: 0, stale: 0, none: 0 })), "This library has no tracks.");
  const t = coverageText(state());
  assert.match(t, /^6 of 10 tracks have an energy estimate\./);
  assert.match(t, /2 not analysed yet\./);
  assert.match(t, /1 file changed since it was analysed\./);
  assert.match(t, /1 too short, silent or under the minimum length/);
  assert.equal(coverageText(state({ total: 1, ready: 1, skipped: 0, stale: 0, none: 0 })), "1 of 1 track has an energy estimate.");
  assert.match(coverageText(state({ stale: 3, none: 0, skipped: 0 })), /3 files changed since they were analysed\./);
  assert.equal(toAnalyse(state()), 3);
});

test("the scale wording names the anchors", () => {
  assert.match(scaleText(state()), /^Fitted to this library: scores from 0\.31 \(energy 1\) to 0\.72 \(energy 10\)/);
  assert.match(scaleText(state({ scale_used: "fixed" })), /^Fixed scale: .*same in every library/);
});

test("the run confirmation names the count and the place", () => {
  assert.equal(runTitle(1, "E:\\", false), "Analyse the energy of 1 track on E:\\ ?");
  assert.equal(runTitle(3, "E:\\", true), "Analyse again 3 tracks on E:\\ ?");
});
