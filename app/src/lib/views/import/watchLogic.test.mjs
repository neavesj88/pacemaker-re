// Run: node --experimental-strip-types --test src/lib/views/import/watchLogic.test.mjs
import assert from "node:assert/strict";
import { test } from "node:test";
import {
  DEFAULT_WATCH_SETTINGS,
  agoText,
  autoDoneText,
  chosenCandidates,
  decide,
  fmtSize,
  intervalLabel,
  logEntryFor,
  looksAbsolute,
  scanLines,
  scanSignature,
  wholeIn,
} from "./watchLogic.ts";

const cand = (path, size = 1000) => ({ path, name: path.split("/").pop(), size, modified_ms: 0, folder: "/m" });
const scanOf = (paths, extra = {}) => ({
  at: 0,
  root: "P:\\",
  folders: [],
  candidates: paths.map((p) => cand(p)),
  candidates_total: paths.length,
  audio_files: paths.length,
  in_library: 0,
  matched_total: 0,
  matched: [],
  ignored: 0,
  too_new: 0,
  duplicates: 0,
  unusable_names: 0,
  cut_short: false,
  ...extra,
});
const base = (over = {}) => ({
  reason: "open",
  settings: { ...DEFAULT_WATCH_SETTINGS, folders: ["/m"] },
  scan: scanOf(["/m/a.mp3", "/m/b.mp3"]),
  canWrite: true,
  tried: new Set(),
  announced: false,
  ...over,
});
const auto = (over = {}) => base({ ...over, settings: { ...DEFAULT_WATCH_SETTINGS, folders: ["/m"], mode: "auto", ...(over.settings ?? {}) } });

test("the defaults ask first, scan on open and have no timer (same as the Rust defaults)", () => {
  assert.equal(DEFAULT_WATCH_SETTINGS.mode, "ask");
  assert.equal(DEFAULT_WATCH_SETTINGS.scan_on_open, true);
  assert.equal(DEFAULT_WATCH_SETTINGS.interval_minutes, 0);
  assert.equal(DEFAULT_WATCH_SETTINGS.auto_max_files, 100);
  assert.equal(DEFAULT_WATCH_SETTINGS.min_age_secs, 10);
  assert.equal(DEFAULT_WATCH_SETTINGS.open_confirmation, false);
  assert.deepEqual(DEFAULT_WATCH_SETTINGS.folders, []);
});

test("ask mode never adds: it tells the user, once per set of files", () => {
  const a = decide(base());
  assert.equal(a.kind, "toast");
  assert.match(a.text, /2 new audio files/);
  assert.match(a.text, /Open Import/);
  assert.equal(decide(base({ announced: true })).kind, "none", "same news again is dropped");
  assert.equal(decide(base({ reason: "timer", announced: true })).kind, "none");
  assert.equal(decide(base({ reason: "manual" })).kind, "none", "a manual scan shows on screen already");
  const one = decide(base({ scan: scanOf(["/m/a.mp3"]) }));
  assert.match(one.text, /1 new audio file found/);
  assert.match(one.text, /review it/);
});

test("ask mode can open the confirmation itself, never skipping it", () => {
  const a = decide(base({ settings: { ...DEFAULT_WATCH_SETTINGS, folders: ["/m"], open_confirmation: true } }));
  assert.deepEqual(a, { kind: "confirm", paths: ["/m/a.mp3", "/m/b.mp3"], announce: true });
  // a manual scan asks again even if the user was told before
  const m = decide(base({ reason: "manual", announced: true, settings: { ...DEFAULT_WATCH_SETTINGS, open_confirmation: true } }));
  assert.equal(m.kind, "confirm");
  // an unattended repeat does not nag
  assert.equal(decide(base({ reason: "timer", announced: true, settings: { ...DEFAULT_WATCH_SETTINGS, open_confirmation: true } })).kind, "none");
});

test("nothing found, or only a refresh, does nothing", () => {
  assert.equal(decide(base({ scan: scanOf([]) })).kind, "none");
  assert.equal(decide(auto({ scan: scanOf([]) })).kind, "none");
  assert.equal(decide(base({ reason: "refresh" })).kind, "none");
  assert.equal(decide(auto({ reason: "refresh" })).kind, "none", "a refresh after a settings change never auto-adds");
});

test("auto mode adds the files, but not more than the limit", () => {
  assert.deepEqual(decide(auto()), { kind: "auto", paths: ["/m/a.mp3", "/m/b.mp3"] });
  const over = decide(auto({ settings: { auto_max_files: 1 } }));
  assert.equal(over.kind, "toast");
  assert.equal(over.tone, "bad");
  assert.match(over.text, /more than the 1 allowed/);
  assert.match(over.text, /nothing was added/);
  // exactly at the limit is allowed
  assert.equal(decide(auto({ settings: { auto_max_files: 2 } })).kind, "auto");
  // the limit counts the whole find, not the listed part
  const big = scanOf(["/m/a.mp3"], { candidates_total: 500 });
  assert.equal(decide(auto({ scan: big })).kind, "toast");
  assert.equal(decide(auto({ scan: big, announced: true, reason: "timer" })).kind, "none");
  assert.equal(decide(auto({ scan: big, announced: true, reason: "manual" })).kind, "toast");
});

test("auto mode does not retry a file it already tried this session", () => {
  const a = decide(auto({ tried: new Set(["/m/a.mp3"]) }));
  assert.deepEqual(a, { kind: "auto", paths: ["/m/b.mp3"] });
  assert.equal(decide(auto({ tried: new Set(["/m/a.mp3", "/m/b.mp3"]) })).kind, "none");
});

test("auto mode waits, and says so once, when the device cannot be written to", () => {
  const a = decide(auto({ canWrite: false }));
  assert.equal(a.kind, "toast");
  assert.match(a.text, /nothing can be added right now/);
  assert.equal(decide(auto({ canWrite: false, announced: true, reason: "timer" })).kind, "none");
});

test("the signature changes when the set of files does", () => {
  const a = scanSignature(scanOf(["/m/a.mp3", "/m/b.mp3"]));
  assert.equal(scanSignature(scanOf(["/m/a.mp3", "/m/b.mp3"])), a);
  assert.notEqual(scanSignature(scanOf(["/m/a.mp3", "/m/c.mp3"])), a);
  assert.notEqual(scanSignature(scanOf(["/m/a.mp3"])), a);
  assert.equal(scanSignature(scanOf([])), "");
});

test("auto results are worded and logged", () => {
  assert.equal(autoDoneText("2 added, 1 failed", 1, null), "Watch folder: 2 added, 1 failed. Open Import to see what failed.");
  assert.equal(autoDoneText("3 added", 0, null), "Watch folder: 3 added.");
  assert.equal(autoDoneText("", 0, null), "Watch folder: nothing to do.");
  assert.match(autoDoneText("x", 0, "disk is full"), /failed: disk is full/);
  const e = logEntryFor("auto", ["a.mp3", "b.mp3"], { ok: 1, failed: 1, summary: "1 added, 1 failed", error: null });
  assert.deepEqual(e, { mode: "auto", requested: 2, names: ["a.mp3", "b.mp3"], ok: 1, failed: 1, summary: "1 added, 1 failed" });
  assert.equal(logEntryFor("auto", ["a"], { ok: 0, failed: 0, summary: "", error: "boom" }).summary, "failed: boom");
});

test("scan lines name only what happened", () => {
  assert.deepEqual(scanLines(scanOf([])), []);
  const lines = scanLines(scanOf([], { in_library: 1, matched_total: 2, ignored: 3, too_new: 1, duplicates: 2, unusable_names: 1, cut_short: true }));
  assert.equal(lines.length, 7);
  assert.match(lines[0], /^1 file is already in the library/);
  assert.match(lines[1], /^2 files left out/);
  assert.match(lines[4], /^2 extra copies/);
  assert.match(lines[6], /more than a scan can look at/);
});

test("sizes, ages and intervals read naturally", () => {
  assert.equal(fmtSize(900), "900 B");
  assert.equal(fmtSize(5 * 1024), "5.0 KB");
  assert.equal(fmtSize(5 * 1024 * 1024), "5.0 MB");
  assert.equal(fmtSize(3 * 1024 ** 3), "3.00 GB");
  const now = 10_000_000_000;
  assert.equal(agoText(now - 5_000, now), "just now");
  assert.equal(agoText(now + 5_000, now), "just now", "a clock slightly ahead is not negative");
  assert.equal(agoText(now - 60_000, now), "1 minute ago");
  assert.equal(agoText(now - 15 * 60_000, now), "15 minutes ago");
  assert.equal(agoText(now - 3 * 3_600_000, now), "3 hours ago");
  assert.equal(agoText(now - 2 * 86_400_000, now), "2 days ago");
  assert.equal(intervalLabel(0), "off");
  assert.equal(intervalLabel(1), "every minute");
  assert.equal(intervalLabel(15), "every 15 minutes");
  assert.equal(intervalLabel(60), "every hour");
  assert.equal(intervalLabel(120), "every 2 hours");
  assert.equal(intervalLabel(90), "every 90 minutes");
});

test("number fields accept only whole numbers in range", () => {
  assert.equal(wholeIn("15", 0, 1440), 15);
  assert.equal(wholeIn(" 0 ", 0, 1440), 0);
  assert.equal(wholeIn("1441", 0, 1440), null);
  assert.equal(wholeIn("-1", 0, 10), null);
  assert.equal(wholeIn("1.5", 0, 10), null);
  assert.equal(wholeIn("", 0, 10), null);
  assert.equal(wholeIn("1e3", 0, 5000), null);
});

test("folder paths must be full paths", () => {
  assert.ok(looksAbsolute("C:\\Music"));
  assert.ok(looksAbsolute("d:/music"));
  assert.ok(looksAbsolute("\\\\nas\\share\\music"));
  assert.ok(looksAbsolute("/home/dj/music"));
  assert.ok(!looksAbsolute("music"));
  assert.ok(!looksAbsolute("..\\music"));
  assert.ok(!looksAbsolute(""));
});

test("chosen candidates keep the list order", () => {
  const all = ["/m/a", "/m/b", "/m/c"].map((p) => cand(p));
  assert.deepEqual(chosenCandidates(all, new Set(["/m/c", "/m/a"])).map((c) => c.path), ["/m/a", "/m/c"]);
  assert.deepEqual(chosenCandidates(all, new Set()), []);
});
