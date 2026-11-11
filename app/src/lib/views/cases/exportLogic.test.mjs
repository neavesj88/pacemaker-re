// Run: node --experimental-strip-types --test src/lib/views/cases/exportLogic.test.mjs
// The sentences and totals of the export-to-a-folder dialog, the first selection, and which
// choices are remembered. Synthetic names only.
import assert from "node:assert/strict";
import { test } from "node:test";
import { actionText, afterQuickExport, confirmLabel, defaultSelection, dirName, fmtBytes, joinPath, planSummary, plural, prefsToSave, replaceList, requestOf, rows, saveSuggestion, totals } from "../../state/exportLogic.ts";

const prefs = (over = {}) => ({ folder: "D:\\Sets", format: "m3u8", relative_paths: true, copy_audio: false, audio_layout: "shared", audio_names: "tags", conflict: "skip", remember: true, ...over });
const list = (over = {}) => ({ name: "Sample Case", smart: false, file: "Sample Case.m3u8", path: "D:\\Sets\\Sample Case.m3u8", tracks: 3, action: "write", wanted: null, ...over });
const audio = (over = {}) => ({ title: "Sample Title", file: "Audio/Sample Artist - Sample Title.mp3", path: "D:\\Sets\\Audio\\x.mp3", size: 5_000_000, action: "write", wanted: null, used_by: 1, problem: null, ...over });
const plan = (lists = [], audioFiles = []) => ({ folder: "D:\\Sets", lists, audio: audioFiles, notes: [], copy_bytes: 0 });

test("sizes read like a file manager", () => {
  assert.equal(fmtBytes(0), "0 B");
  assert.equal(fmtBytes(999), "999 B");
  assert.equal(fmtBytes(1500), "1.5 KB");
  assert.equal(fmtBytes(12_345_678), "12.3 MB");
  assert.equal(fmtBytes(340_000_000), "340 MB");
  assert.equal(fmtBytes(1_234_000_000), "1.2 GB");
  assert.equal(plural(1, "file"), "1 file");
  assert.equal(plural(2, "file"), "2 files");
});

test("paths are split and joined with the separator already in use", () => {
  assert.equal(dirName("D:\\Sets\\a.m3u8"), "D:\\Sets");
  assert.equal(dirName("D:\\a.m3u8"), "D:\\");
  assert.equal(dirName("/home/x/a.m3u8"), "/home/x");
  assert.equal(dirName("/a.m3u8"), "/");
  assert.equal(dirName("a.m3u8"), "");
  assert.equal(joinPath("D:\\Sets", "a.m3u8"), "D:\\Sets\\a.m3u8");
  assert.equal(joinPath("/home/x", "a.m3u8"), "/home/x/a.m3u8");
  assert.equal(joinPath("D:\\", "a.m3u8"), "D:\\a.m3u8");
  assert.equal(joinPath("", "a.m3u8"), "a.m3u8");
  assert.equal(saveSuggestion({ folder: "D:\\Sets" }, "Warm-up: 1/2", "m3u8"), "D:\\Sets\\Warm-up_ 1_2.m3u8");
  assert.equal(saveSuggestion({ folder: null }, "  ", "csv"), "case.csv");
});

test("every action says in words what happens to the file", () => {
  assert.equal(actionText("write", "list"), "New file");
  assert.equal(actionText("replace", "audio"), "Replaces the file that is there");
  assert.match(actionText("renamed", "list", "A.m3u8"), /is kept; saved as this instead \(wanted “A.m3u8”\)/);
  assert.match(actionText("skip", "list"), /left as it is, not written/);
  assert.match(actionText("skip", "audio"), /not copied \(the playlist keeps pointing at the original\)/);
  assert.match(actionText("same", "audio"), /same size/);
  assert.equal(actionText("missing", "audio", null, "the audio file is missing from the device"), "Cannot be copied: the audio file is missing from the device");
  assert.match(actionText("blocked", "audio", null, "a folder with that name is in the way"), /Not written: a folder/);
});

test("totals count only what is written and the button says exactly that", () => {
  const p = plan(
    [list(), list({ file: "B.m3u8", action: "replace" }), list({ file: "C.m3u8", action: "skip" }), list({ file: "D (2).m3u8", action: "renamed", wanted: "D.m3u8" })],
    [audio(), audio({ size: 1_000_000, action: "replace" }), audio({ action: "same" }), audio({ action: "skip" }), audio({ action: "missing", size: 0 })],
  );
  assert.deepEqual(totals(p), { lists: 3, audio: 2, bytes: 6_000_000, replaced: 2, renamed: 1, kept: 2, same: 1, problems: 1 });
  assert.equal(confirmLabel(p), "Write 3 playlists and copy 2 audio files (6.0 MB)");
  assert.equal(confirmLabel(plan([list()])), "Write 1 playlist");
  assert.equal(confirmLabel(plan([list({ action: "skip" })])), "Nothing to write");
  assert.equal(confirmLabel(plan([list({ action: "skip" })], [audio()])), "Copy 1 audio file (5.0 MB)");
  assert.equal(planSummary(p), "2 existing files will be replaced; 1 file will get a numbered name because the name is taken; 2 existing files will be left as they are; 1 audio file is already there; 1 file cannot be written");
  assert.equal(planSummary(plan([list()])), "");
});

test("a replace names every file it would write over, playlists and audio", () => {
  const p = plan([list({ file: "B.m3u8", action: "replace" }), list()], [audio({ file: "Audio/x.mp3", action: "replace" })]);
  assert.deepEqual(replaceList(p), ["B.m3u8", "Audio/x.mp3"]);
  assert.deepEqual(replaceList(plan([list()])), []);
});

test("the first selection leaves out the Pacemaker copy of a smart case", () => {
  const infos = [{ id: 1 }, { id: 2 }, { id: 3 }];
  const smart = [{ id: 10, device_case_id: 2 }, { id: 11, device_case_id: null }];
  assert.deepEqual(defaultSelection(infos, smart), { caseIds: [1, 3], smartIds: [10, 11] });
  assert.deepEqual(defaultSelection(infos, []), { caseIds: [1, 2, 3], smartIds: [] });
});

test("the request carries the choices, and relative paths only apply to M3U8", () => {
  const r = requestOf(prefs({ folder: "  D:\\Sets " }), new Set([2, 1]), [5]);
  assert.deepEqual(r, { folder: "D:\\Sets", format: "m3u8", case_ids: [2, 1], smart_ids: [5], relative_paths: true, copy_audio: false, audio_layout: "shared", audio_names: "tags", conflict: "skip" });
  assert.equal(requestOf(prefs({ format: "csv" }), [], []).relative_paths, false);
  assert.equal(requestOf(prefs({ folder: null }), [], []).folder, "");
});

test("folder and format are remembered only when asked; the policies always are", () => {
  const stored = prefs({ folder: "D:\\Old", format: "csv", conflict: "skip" });
  const draft = prefs({ folder: "D:\\New", format: "rekordbox", conflict: "number", remember: true });
  assert.deepEqual(prefsToSave(draft, stored), draft);
  const off = prefsToSave({ ...draft, remember: false }, stored);
  assert.deepEqual([off.folder, off.format, off.conflict], ["D:\\Old", "csv", "number"]);
});

test("a one-file export moves the remembered folder and format to where it went", () => {
  const stored = prefs({ folder: "D:\\Old", format: "m3u8" });
  assert.deepEqual(afterQuickExport(stored, "E:\\Other\\a.csv", "csv"), { ...stored, folder: "E:\\Other", format: "csv" });
  assert.equal(afterQuickExport(stored, "D:\\Old\\a.m3u8", "m3u8"), null, "nothing changed");
  assert.equal(afterQuickExport({ ...stored, remember: false }, "E:\\Other\\a.csv", "csv"), null);
});

test("the preview rows name every file with its words", () => {
  const p = plan([list({ smart: true, name: "Rule", tracks: 1 })], [audio({ used_by: 2 })]);
  const r = rows(p);
  assert.equal(r.length, 2);
  assert.deepEqual([r[0].kind, r[0].file, r[0].detail], ["list", "Sample Case.m3u8", "Smart case “Rule” · 1 track"]);
  assert.deepEqual([r[1].kind, r[1].detail], ["audio", "Sample Title · 5.0 MB · in 2 playlists"]);
});
