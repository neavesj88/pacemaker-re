// node --experimental-strip-types src/lib/views/analysis/sectionLogic.test.mjs
import assert from "node:assert/strict";
import test from "node:test";
import {
  boundariesOf, clampSetting, clock, confidenceOpacity, coverageText, coveredSecs, DEFAULT_SETTINGS, entriesById, KIND_LABEL, KINDS, kindColor, LIMITS,
  neighbourBoundary, phraseText, runTitle, sameSettings, sectionAt, sectionTitle, skipHint, snapToBoundary, summaryTitle, toAnalyse,
} from "./sectionLogic.ts";

const sec = (kind, start_bar, end_bar, over = {}) => ({
  kind, start_bar, end_bar, start_secs: start_bar * 2, end_secs: end_bar * 2, confidence: 0.8, start_confidence: 0.9, on_phrase: true, ...over,
});
// intro 0-16, build 16-24, drop 24-56, breakdown 56-72, drop 72-96, outro 96-104 (2 s a bar)
const SECTIONS = [sec("intro", 0, 16, { start_confidence: 1 }), sec("build", 16, 24, { start_confidence: 0.4 }), sec("drop", 24, 56), sec("breakdown", 56, 72, { on_phrase: false }), sec("drop", 72, 96), sec("outro", 96, 104)];

const state = (over = {}) => ({
  entries: [], total: 10, ready: 5, flat: 1, skipped: 1, skip_reasons: [], stale: 1, none: 2, settings: DEFAULT_SETTINGS, problem: null, ...over,
});

test("settings are clamped to the backend's ranges, whole numbers, NaN to the default", () => {
  assert.equal(clampSetting("sensitivity", 9), 5);
  assert.equal(clampSetting("sensitivity", 0), 1);
  assert.equal(clampSetting("min_section_bars", 3), 4);
  assert.equal(clampSetting("min_section_bars", 99), 32);
  assert.equal(clampSetting("min_section_bars", 7.6), 8);
  assert.equal(clampSetting("snap_tolerance_bars", -1), 0);
  assert.equal(clampSetting("snap_tolerance_bars", 12), 8);
  assert.equal(clampSetting("min_bars", 4), 16);
  assert.equal(clampSetting("min_bars", 4000), 512);
  assert.equal(clampSetting("threads", 0), 1);
  assert.equal(clampSetting("threads", 99), 8);
  assert.equal(clampSetting("threads", NaN), DEFAULT_SETTINGS.threads);
  assert.equal(clampSetting("min_bars", Infinity), DEFAULT_SETTINGS.min_bars);
});

test("the defaults sit inside their own limits and compare equal to a copy", () => {
  for (const k of Object.keys(LIMITS)) assert.equal(clampSetting(k, DEFAULT_SETTINGS[k]), DEFAULT_SETTINGS[k]);
  assert.ok(sameSettings(DEFAULT_SETTINGS, { ...DEFAULT_SETTINGS }));
  assert.ok(!sameSettings(DEFAULT_SETTINGS, { ...DEFAULT_SETTINGS, origin: "best_fit" }));
  assert.ok(!sameSettings(DEFAULT_SETTINGS, { ...DEFAULT_SETTINGS, show_markers: false }));
  assert.ok(!sameSettings(DEFAULT_SETTINGS, { ...DEFAULT_SETTINGS, phrase_bars: 16 }));
});

test("every kind has a label and a colour token", () => {
  for (const k of KINDS) {
    assert.ok(KIND_LABEL[k].length > 0);
    assert.equal(kindColor(k), `var(--sec-${k})`);
  }
  assert.equal(new Set(KINDS).size, 6);
});

test("clock, opacity and the lookups", () => {
  assert.equal(clock(0), "0:00");
  assert.equal(clock(59.6), "1:00");
  assert.equal(clock(125), "2:05");
  assert.equal(clock(-3), "0:00");
  assert.equal(confidenceOpacity(1), 1);
  assert.equal(confidenceOpacity(0), 0.35);
  assert.equal(confidenceOpacity(7), 1);
  assert.equal(confidenceOpacity(-1), 0.35);
  assert.equal(coveredSecs(SECTIONS), 208);
  assert.equal(coveredSecs([]), 0);
  assert.equal(entriesById([{ track_id: 4, sections: [] }, { track_id: 9, sections: [] }]).get(9).track_id, 9);
  assert.equal(sectionAt(SECTIONS, 0).kind, "intro");
  assert.equal(sectionAt(SECTIONS, 31.99).kind, "intro");
  assert.equal(sectionAt(SECTIONS, 32).kind, "build", "a boundary belongs to the section that starts there");
  assert.equal(sectionAt(SECTIONS, 100).kind, "drop");
  assert.equal(sectionAt(SECTIONS, 208).kind, "outro", "the very end belongs to the last section");
  assert.equal(sectionAt(SECTIONS, 209), null);
  assert.equal(sectionAt(SECTIONS, -1), null);
  assert.equal(sectionAt([], 3), null);
});

test("titles name the section, its times, its bars and its confidence", () => {
  assert.equal(sectionTitle(sec("drop", 24, 56)), "Drop, 0:48 to 1:52, bars 25 to 56, confidence 80%");
  assert.equal(summaryTitle(SECTIONS.slice(0, 3)), "Sections (estimated): intro 0:00 (80%), build 0:32 (80%), drop 0:48 (80%)");
  assert.match(phraseText({ phrase_bars: 8, sections: SECTIONS }), /8-bar phrase grid \(4 of 5 on it\)/);
  assert.match(phraseText({ phrase_bars: 0, sections: SECTIONS }), /no phrase grid fits/);
  assert.equal(phraseText({ phrase_bars: 0, sections: SECTIONS.slice(0, 1) }), "");
});

test("boundaries are the starts of every section but the first, and filter", () => {
  const all = boundariesOf(SECTIONS);
  assert.deepEqual(all.map((b) => b.bar), [16, 24, 56, 72, 96]);
  assert.deepEqual(all.map((b) => b.secs), [32, 48, 112, 144, 192]);
  assert.deepEqual(all[2], { secs: 112, bar: 56, before: "drop", after: "breakdown", confidence: 0.9, on_phrase: false });
  assert.deepEqual(boundariesOf(SECTIONS, { into: ["drop"] }).map((b) => b.bar), [24, 72]);
  assert.deepEqual(boundariesOf(SECTIONS, { from: ["drop"] }).map((b) => b.bar), [56, 96]);
  assert.deepEqual(boundariesOf(SECTIONS, { into: ["drop"], from: ["breakdown"] }).map((b) => b.bar), [72]);
  assert.deepEqual(boundariesOf(SECTIONS, { minConfidence: 0.5 }).map((b) => b.bar), [24, 56, 72, 96], "the weak build start is dropped");
  assert.deepEqual(boundariesOf(SECTIONS, { onPhraseOnly: true }).map((b) => b.bar), [16, 24, 72, 96]);
  assert.deepEqual(boundariesOf([]), []);
  assert.deepEqual(boundariesOf(SECTIONS.slice(0, 1)), []);
});

test("snapping finds the nearest boundary within reach, ties to the earlier, filters apply", () => {
  assert.deepEqual(snapToBoundary(SECTIONS, 50), { secs: 48, distance: 2, boundary: boundariesOf(SECTIONS)[1] });
  assert.equal(snapToBoundary(SECTIONS, 112.4).secs, 112);
  assert.equal(snapToBoundary(SECTIONS, 40).secs, 32, "equidistant from 32 and 48: the earlier");
  assert.equal(snapToBoundary(SECTIONS, 70, { maxSecs: 8 }), null, "nothing within 8 s");
  assert.equal(snapToBoundary(SECTIONS, 70, { maxSecs: 50 }).secs, 48);
  assert.equal(snapToBoundary(SECTIONS, 110, { into: ["drop"], maxSecs: 40 }).secs, 144, "the 112 boundary starts a breakdown, so only drops count");
  assert.equal(snapToBoundary(SECTIONS, 110, { into: ["drop"], maxSecs: 20 }), null);
  assert.equal(snapToBoundary(SECTIONS, 0), null, "the start of the track is not a boundary");
  assert.equal(snapToBoundary(SECTIONS, 208), null);
  assert.equal(snapToBoundary(SECTIONS, NaN), null);
  assert.equal(snapToBoundary([], 5), null);
  assert.equal(snapToBoundary(SECTIONS, 34, { minConfidence: 0.5, maxSecs: 20 }).secs, 48, "the weak boundary at 32 is skipped");
});

test("neighbours step through the boundaries", () => {
  assert.equal(neighbourBoundary(SECTIONS, 0, 1).bar, 16);
  assert.equal(neighbourBoundary(SECTIONS, 32, 1).bar, 24, "strictly after");
  assert.equal(neighbourBoundary(SECTIONS, 192, 1), null);
  assert.equal(neighbourBoundary(SECTIONS, 200, -1).bar, 96);
  assert.equal(neighbourBoundary(SECTIONS, 96, -1).bar, 24, "strictly before 96 s is the boundary at 48 s");
  assert.equal(neighbourBoundary(SECTIONS, 30, -1), null);
  assert.equal(neighbourBoundary(SECTIONS, 100, 1, { into: ["drop"] }).bar, 72);
  assert.equal(neighbourBoundary(SECTIONS, 150, -1, { into: ["drop"] }).bar, 72);
});

test("wording", () => {
  const s = state();
  assert.equal(toAnalyse(s), 3);
  const text = coverageText(s);
  assert.match(text, /^5 of 10 tracks have sections\./);
  assert.match(text, /2 not analysed yet\./);
  assert.match(text, /1 file changed since it was analysed\./);
  assert.match(text, /1 analysed but without clear sections/);
  assert.match(text, /1 with nothing to show/);
  assert.equal(coverageText(state({ total: 0, ready: 0, flat: 0, skipped: 0, stale: 0, none: 0 })), "This library has no tracks.");
  assert.equal(coverageText(state({ total: 1, ready: 1, flat: 0, skipped: 0, stale: 0, none: 0 })), "1 of 1 track has sections.");
  assert.equal(runTitle(1, "E:\\", false), "Detect sections in 1 track on E:\\ ?");
  assert.equal(runTitle(3, "E:\\", true), "Analyse again 3 tracks on E:\\ ?");
  assert.match(skipHint("the beat grid has no first downbeat set"), /Grid editor/);
  assert.match(skipHint("no usable beat grid"), /beat grid/);
  assert.match(skipHint("fewer than 32 bars"), /Lower the shortest/);
  assert.equal(skipHint("silent"), "");
});
