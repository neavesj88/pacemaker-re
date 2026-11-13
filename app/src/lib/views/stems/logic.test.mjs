// Run: node --experimental-strip-types --test src/lib/views/stems/logic.test.mjs  (from app/)
import assert from "node:assert/strict";
import { test } from "node:test";
import {
  actionVerb, canStart, clock, componentState, fileName, filesToWrite, formatBytes, formatEta, looksLikeDevice, noun, phaseText,
  planHeadline, resultFiles, runFraction, runnable, sameSettings, settingsProblem, suffixProblem,
} from "./logic.ts";

const item = (over = {}) => ({
  track_id: 1, label: "Demo Artist - Tune", minutes: 4, source: "S", vocals_path: "D/a (Acapella).mp3", instrumental_path: "D/a (Instrumental).mp3",
  vocals_exists: false, instrumental_exists: false, write_vocals: true, write_instrumental: true, action: "make", what: "", problem: null, segments: 40, ...over,
});
const plan = (items, over = {}) => ({
  dest: "D", format_label: "MP3", items, make: 0, replace: 0, partial: 0, skip: 0, refuse: 0, minutes: 0, replaced_files: [], ...over,
});
const settings = () => ({
  device: "auto", threads: 0, format: "mp3", mp3_kbps: 320, mp3_align: true, ogg_quality: 0.6, overlap_percent: 50, instrumental: "sum",
  max_minutes: 30, vocals_suffix: "(Acapella)", instrumental_suffix: "(Instrumental)", existing: "skip", runtime_path: "", last_folder: "",
});

test("byte sizes use decimal units", () => {
  assert.equal(formatBytes(209_884_896), "210 MB");
  assert.equal(formatBytes(82_929_944), "82.9 MB");
  assert.equal(formatBytes(999), "999 B");
  assert.equal(formatBytes(12_345), "12 kB");
  assert.equal(formatBytes(2_500_000_000), "2.5 GB");
  assert.equal(formatBytes(-1), "");
  assert.equal(formatBytes(Number.NaN), "");
});

test("the time left is rounded and never shows false precision", () => {
  assert.equal(formatEta(null), "");
  assert.equal(formatEta(undefined), "");
  assert.equal(formatEta(-3), "");
  assert.equal(formatEta(4), "under 10 s");
  assert.equal(formatEta(42), "about 40 s");
  assert.equal(formatEta(150), "about 3 min");
  assert.equal(formatEta(60 * 100), "about 1 h 40 min");
  assert.equal(clock(125), "2:05");
  assert.equal(clock(-5), "0:00");
});

test("progress runs 0 to 1 over the items and tolerates unknown segment counts", () => {
  const p = (index, segment, segments, count = 4) => ({ phase: "separating", index, count, label: "", segment, segments, eta_secs: null, device: "CPU" });
  assert.equal(runFraction(null), 0);
  assert.equal(runFraction(p(0, 0, 0)), 0);
  assert.equal(runFraction(p(1, 10, 20)), 0.375);
  assert.equal(runFraction(p(3, 20, 20)), 1);
  assert.equal(runFraction(p(9, 99, 20)), 1, "clamped");
  assert.equal(runFraction({ ...p(0, 0, 0), count: 0 }), 0);
});

test("phase text names the track and the part", () => {
  assert.equal(phaseText(null), "Starting…");
  assert.equal(phaseText({ phase: "loading-model", index: 0, count: 0, label: "", segment: 0, segments: 0, eta_secs: null, device: "" }), "Loading the model…");
  const sep = { phase: "separating", index: 1, count: 3, label: "x", segment: 4, segments: 30, eta_secs: 60, device: "CPU" };
  assert.equal(phaseText(sep), "track 2 of 3, part 5 of 30");
  assert.equal(phaseText({ ...sep, count: 1, index: 0, segment: 0, segments: 0 }), "reading the file");
});

test("a part's state says what is missing", () => {
  const c = { available_here: true, installed: false, size_bytes: 82_929_944, installed_bytes: 0 };
  assert.deepEqual(componentState(c), { text: "Not downloaded (82.9 MB)", tone: "todo" });
  assert.deepEqual(componentState({ ...c, installed: true, installed_bytes: 209_884_896 }), { text: "Installed (210 MB)", tone: "ok" });
  assert.equal(componentState({ ...c, available_here: false }).tone, "na");
});

test("the plan headline counts every kind of item", () => {
  const p = plan([], { make: 2, replace: 1, partial: 1, skip: 3, refuse: 1, minutes: 16.4, replaced_files: ["a", "b"] });
  assert.equal(planHeadline(p), "4 tracks to separate (16 min of audio); 2 existing files will be replaced; 3 tracks already done and kept; 1 track cannot be done");
  assert.equal(planHeadline(plan([])), "Nothing to do");
  assert.equal(planHeadline(plan([], { make: 1, minutes: 3 })), "1 track to separate (3 min of audio)");
  assert.equal(noun(1, "file"), "1 file");
  assert.equal(noun(2, "file"), "2 files");
});

test("starting needs something to run, and replacing needs the tick", () => {
  assert.equal(canStart(null, true), false);
  assert.equal(canStart(plan([item({ action: "skip" })], { skip: 1 }), true), false, "nothing would run");
  assert.equal(canStart(plan([item()], { make: 1 }), false), true);
  const r = plan([item({ action: "replace", vocals_exists: true })], { replace: 1, replaced_files: ["D/a (Acapella).mp3"] });
  assert.equal(canStart(r, false), false);
  assert.equal(canStart(r, true), true);
  assert.equal(actionVerb("replace"), "Replace");
});

test("the files to write follow the per-file flags", () => {
  const p = plan([
    item(),
    item({ track_id: 2, action: "partial", write_vocals: false, vocals_path: "D/b (Acapella).mp3", instrumental_path: "D/b (Instrumental).mp3" }),
    item({ track_id: 3, action: "skip", write_vocals: false, write_instrumental: false }),
    item({ track_id: 4, action: "refuse", write_vocals: false, write_instrumental: false }),
  ]);
  assert.deepEqual(filesToWrite(p), ["D/a (Acapella).mp3", "D/a (Instrumental).mp3", "D/b (Instrumental).mp3"]);
  assert.equal(runnable(p).length, 2);
});

test("a result lists the written files, vocals then instrumental, dropping the kept ones", () => {
  assert.deepEqual(resultFiles(null), []);
  const r = {
    dest: "D", device: "CPU", skipped: [], failed: [], cancelled: false, limited_samples: 0,
    written: [{ track_id: 1, label: "a", vocals: "V1", instrumental: "I1", seconds: 1 }, { track_id: 2, label: "b", vocals: null, instrumental: "I2", seconds: 1 }],
  };
  assert.deepEqual(resultFiles(r), ["V1", "I1", "I2"]);
  assert.equal(fileName("C:\\Stems\\a (Acapella).mp3"), "a (Acapella).mp3");
  assert.equal(fileName("/x/y.mp3"), "y.mp3");
});

test("suffix and settings checks match the backend's rules", () => {
  assert.equal(suffixProblem("S", "(Acapella)"), null);
  assert.match(suffixProblem("S", "  "), /1 to 40/);
  assert.match(suffixProblem("S", "x".repeat(41)), /1 to 40/);
  for (const bad of ["a/b", "a\\b", "a:b", "a*b", "a?b", 'a"b', "a<b", "a>b", "a|b", "a\tb"]) assert.match(suffixProblem("S", bad), /cannot contain/, bad);
  assert.equal(settingsProblem(settings()), null);
  assert.match(settingsProblem({ ...settings(), instrumental_suffix: "(acapella)" }), /must differ/);
  assert.match(settingsProblem({ ...settings(), overlap_percent: 95 }), /10 and 90/);
  assert.match(settingsProblem({ ...settings(), vocals_suffix: "" }), /vocals suffix/);
});

test("settings compare by value and ignore the consent records", () => {
  assert.equal(sameSettings(settings(), { ...settings(), consents: { a: { notice_version: 1, accepted_at: 5 } } }), true);
  assert.equal(sameSettings(settings(), { ...settings(), mp3_kbps: 256 }), false);
});

test("a destination on the device is recognised before the backend is asked", () => {
  assert.equal(looksLikeDevice("P:\\", "P:\\"), true);
  assert.equal(looksLikeDevice("p:\\Stems", "P:\\"), true);
  assert.equal(looksLikeDevice("P:\\Stems\\", "P:"), true);
  assert.equal(looksLikeDevice("C:\\Stems", "P:\\"), false);
  assert.equal(looksLikeDevice("P:\\Stems", undefined), false);
  assert.equal(looksLikeDevice("/mnt/pm-extra", "/mnt/pm"), false, "a sibling with the same prefix is not inside");
});
