// Run: node --experimental-strip-types --test src/lib/views/vdj/vdjLogic.test.mjs
import assert from "node:assert/strict";
import { test } from "node:test";
import { DEFAULT_OPTIONS, buildRequest, cleanOptions, entryWords, formatBytes, planLine, selectedInCase, startGate, startLabel, suggestListName } from "./vdjLogic.ts";

const entry = (o) => ({ track_id: 1, label: "Example Artist - Demo Track 01", status: "copy", file: "Example Artist - Demo Track 01.mp3", path: "D:\\x.mp3", size: 5_000_000, reason: "", ...o });
const plan = (o = {}) => ({
  list_name: "Set A", folder: "E:\\Gigs\\Set A", list_file: "E:\\Gigs\\Set A\\Set A.xml", list_exists: false, copy_audio: true,
  entries: [], copy_count: 2, reuse_count: 1, reference_count: 0, left_out_count: 1, copy_bytes: 10_000_000, notes: [], ...o,
});

test("saved options are checked field by field and fall back to the defaults", () => {
  assert.deepEqual(cleanOptions(undefined), DEFAULT_OPTIONS);
  assert.deepEqual(cleanOptions(null), DEFAULT_OPTIONS);
  assert.deepEqual(cleanOptions("junk"), DEFAULT_OPTIONS);
  assert.deepEqual(cleanOptions({ copy_audio: false, name_files: "original", ordered: false, no_duplicates: false, path_base: "D:\\Gig" }),
    { copy_audio: false, name_files: "original", ordered: false, no_duplicates: false, path_base: "D:\\Gig" });
  const o = cleanOptions({ copy_audio: "yes", name_files: "weird", ordered: 1, path_base: 7 });
  assert.deepEqual(o, DEFAULT_OPTIONS);
  assert.equal(cleanOptions({ path_base: "x".repeat(900) }).path_base.length, 500);
});

test("the defaults are the safe, self-contained choices", () => {
  assert.equal(DEFAULT_OPTIONS.copy_audio, true);
  assert.equal(DEFAULT_OPTIONS.ordered, true);
  assert.equal(DEFAULT_OPTIONS.path_base, "");
});

test("byte sizes read like a file manager", () => {
  assert.equal(formatBytes(0), "0 B");
  assert.equal(formatBytes(-5), "0 B");
  assert.equal(formatBytes(NaN), "0 B");
  assert.equal(formatBytes(512), "512 B");
  assert.equal(formatBytes(1024), "1.0 kB");
  assert.equal(formatBytes(5_000_000), "4.8 MB");
  assert.equal(formatBytes(340 * 1024 * 1024), "340 MB");
  assert.equal(formatBytes(1.5 * 1024 ** 3), "1.5 GB");
  assert.equal(formatBytes(5000 * 1024 ** 4), "5000 TB");
});

test("every outcome has a sentence that says what will happen", () => {
  assert.match(entryWords(entry()), /^Copied as Example Artist - Demo Track 01\.mp3 \(4\.8 MB\)$/);
  assert.match(entryWords(entry({ status: "reuse", file: "a.mp3" })), /already in the folder: kept as it is, not copied again/);
  assert.match(entryWords(entry({ status: "reference" })), /not copied/);
  assert.equal(entryWords(entry({ status: "left_out", reason: "The audio file is missing from the Pacemaker." })), "Left out of the list: The audio file is missing from the Pacemaker.");
  assert.match(entryWords(entry({ status: "left_out", reason: "" })), /cannot be exported/);
});

test("the summary counts what is listed and what is left out", () => {
  assert.equal(planLine(plan()), "The list holds 3 tracks: copies 2 files (9.5 MB), keeps 1 already in the folder, leaves out 1.");
  assert.equal(planLine(plan({ copy_count: 1, reuse_count: 0, left_out_count: 0, copy_bytes: 1024 })), "The list holds 1 track: copies 1 file (1.0 kB).");
  assert.equal(planLine(plan({ copy_audio: false, copy_count: 0, reuse_count: 0, reference_count: 4, left_out_count: 0, copy_bytes: 0 })),
    "The list holds 4 tracks: points 4 tracks at the Pacemaker, copies nothing.");
  assert.equal(startLabel(null), "Export");
  assert.equal(startLabel(plan()), "Export 3 tracks");
  assert.equal(startLabel(plan({ copy_count: 1, reuse_count: 0 })), "Export 1 track");
});

test("Export is allowed only when the plan is ready and any replacement is agreed", () => {
  const ok = { plan: plan(), planError: "", planning: false, replace: false, running: false, folder: "E:\\Gigs" };
  assert.deepEqual(startGate(ok), { ok: true, why: "" });
  assert.equal(startGate({ ...ok, running: true }).ok, false);
  assert.match(startGate({ ...ok, folder: "  " }).why, /Choose the folder/);
  assert.equal(startGate({ ...ok, planError: "Give the list a name." }).why, "Give the list a name.");
  assert.match(startGate({ ...ok, planning: true }).why, /Working out/);
  assert.match(startGate({ ...ok, plan: null }).why, /Working out/);
  assert.match(startGate({ ...ok, plan: plan({ copy_count: 0, reuse_count: 0, reference_count: 0, left_out_count: 3 }) }).why, /None of these tracks/);
  const exists = { ...ok, plan: plan({ list_exists: true }) };
  assert.equal(startGate(exists).ok, false);
  assert.match(startGate(exists).why, /replace the existing list file/);
  assert.equal(startGate({ ...exists, replace: true }).ok, true);
});

test("the selection is narrowed to the case, in case order, without repeats", () => {
  assert.deepEqual(selectedInCase([5, 2, 9], [1, 2, 3, 5, 5]), [2, 5]);
  assert.deepEqual(selectedInCase([], [1, 2]), []);
  assert.deepEqual(selectedInCase([1], []), []);
});

test("list names start from the case name", () => {
  assert.equal(suggestListName("  Warm-up   set "), "Warm-up set");
  assert.equal(suggestListName("   "), "Pacemaker case");
});

test("the request carries the trimmed fields and drops the path base when nothing is copied", () => {
  const base = { root: "P:\\", caseId: 4, onlyTrackIds: [3, 1], listName: "  Set A ", destDir: " E:\\Gigs ", options: { ...DEFAULT_OPTIONS, path_base: " D:\\Gigs\\Set A " } };
  const r = buildRequest(base);
  assert.deepEqual(r, { root: "P:\\", case_id: 4, only_track_ids: [3, 1], list_name: "Set A", dest_dir: "E:\\Gigs", options: { ...DEFAULT_OPTIONS, path_base: "D:\\Gigs\\Set A" } });
  const ref = buildRequest({ ...base, options: { ...base.options, copy_audio: false } });
  assert.equal(ref.options.path_base, "");
  assert.equal(base.options.path_base, " D:\\Gigs\\Set A ", "the dialog's own options are not changed");
});
