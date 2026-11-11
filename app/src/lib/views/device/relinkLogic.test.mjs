// Run: node --experimental-strip-types --test src/lib/views/device/relinkLogic.test.mjs
import assert from "node:assert/strict";
import { test } from "node:test";
import {
  actionLine, cardMusicFolder, confidenceLabel, confirmLine, freeSpaceProblem, isPreselected, kindLabel, locationLine, matchSummary, scanLine,
  scanPercent, sizeText, sortRows, tickAfterBulk, nonStandardNote, totals, totalsLine,
} from "./relinkLogic.ts";

const MB = 1024 * 1024;

test("sizes read like the Storage card", () => {
  assert.equal(sizeText(512), "512 B");
  assert.equal(sizeText(1536), "1.5 KB");
  assert.equal(sizeText(5 * MB), "5.0 MB");
  assert.equal(sizeText(3 * 1024 * MB), "3.0 GB");
});

test("the pre-tick setting decides which matches start ticked, and a hand pick never does", () => {
  const sure = { confidence: "sure", kind: "exact" };
  const likely = { confidence: "likely", kind: "same_name" };
  const check = { confidence: "check", kind: "same_name" };
  const manual = { confidence: "check", kind: "manual" };
  assert.deepEqual(["none", "sure", "likely"].map((m) => isPreselected(sure, m)), [false, true, true]);
  assert.deepEqual(["none", "sure", "likely"].map((m) => isPreselected(likely, m)), [false, false, true]);
  assert.deepEqual(["none", "sure", "likely"].map((m) => isPreselected(check, m)), [false, false, false], "a Check match is always the user's call");
  assert.equal(isPreselected(manual, "likely"), false);
  assert.equal(isPreselected(undefined, "likely"), false);
});

test("labels say what the evidence is", () => {
  assert.equal(confidenceLabel("sure"), "Sure");
  assert.equal(confidenceLabel("check"), "Check");
  assert.equal(kindLabel("exact"), "Same file name and size");
  assert.equal(kindLabel("manual"), "Picked by you");
});

test("each action is described by what it does to the card", () => {
  assert.equal(actionLine({ action: "copy", dest: "6/8/722ef9a6.mp3", on_card: false }), "Copies the file onto the card as 6/8/722ef9a6.mp3");
  assert.match(actionLine({ action: "copy", dest: "6/8/x.mp3", on_card: true }), /already on the card, under another name/);
  assert.equal(
    actionLine({ action: "point", dest: "misc/a.mp3", on_card: true }),
    "Points the track at misc/a.mp3, where the file already is; nothing is copied",
  );
});

test("totals separate copies from re-pointed tracks", () => {
  const t = totals([{ action: "copy", size: 5 * MB }, { action: "point", size: 9 * MB }, { action: "copy", size: MB }]);
  assert.deepEqual(t, { copies: 2, copyBytes: 6 * MB, points: 1 });
  assert.equal(totalsLine(t), "This will copy 2 files (6.0 MB) onto the card and point 1 track at files already on the card.");
  assert.equal(totalsLine(totals([{ action: "point", size: 1 }])), "This will point 1 track at files already on the card.");
  assert.equal(totalsLine(totals([])), "Nothing is ticked.");
});

test("free space: copies need their size plus a margin, unknown space is left to the backend", () => {
  assert.equal(freeSpaceProblem(0, 10), "");
  assert.equal(freeSpaceProblem(50 * MB, null), "");
  assert.equal(freeSpaceProblem(50 * MB, 0), "", "free space that could not be read is not a refusal");
  assert.equal(freeSpaceProblem(50 * MB, 100 * MB), "");
  assert.equal(freeSpaceProblem(50 * MB, 55 * MB), "The card has 55.0 MB free and the copies need 58.0 MB.");
});

test("the confirmation names the file and where it goes", () => {
  assert.equal(
    confirmLine("Example Artist A - Demo Track 07", { action: "copy", path: "C:\\Music\\a.mp3", dest: "6/8/722ef9a6.mp3", size: 5 * MB }),
    "Example Artist A - Demo Track 07: copy C:\\Music\\a.mp3 (5.0 MB) to .Pacemaker/Music/6/8/722ef9a6.mp3",
  );
  assert.equal(confirmLine("T", { action: "point", path: "x", dest: "misc/a.mp3", size: 1 }), "T: use the file already at .Pacemaker/Music/misc/a.mp3");
});

test("the library path is shown before and after", () => {
  assert.equal(locationLine("/pmdb_tracks/6/8/a.mp3", "/pmdb_tracks/6/8/A.MP3"), "The track keeps its library path /pmdb_tracks/6/8/A.MP3");
  assert.equal(
    locationLine("/pmdb_tracks/6/8/a.mp3", "/pmdb_tracks/misc/b.mp3"),
    "The track's library path changes from /pmdb_tracks/6/8/a.mp3 to /pmdb_tracks/misc/b.mp3",
  );
});

test("the headline counts matches and how many are sure", () => {
  assert.equal(matchSummary(0, 0, 0), "No track is missing its audio file.");
  assert.equal(matchSummary(3, 0, 0), "None of the 3 missing tracks has a match in this folder.");
  assert.equal(matchSummary(1, 1, 1), "1 of 1 missing track has a match; all of them are sure.");
  assert.equal(matchSummary(4, 3, 1), "3 of 4 missing tracks have a match; 1 is sure.");
  assert.equal(matchSummary(5, 4, 2), "4 of 5 missing tracks have a match; 2 are sure.");
});

test("search progress reads naturally and the bar only moves while comparing", () => {
  assert.equal(scanLine(null), "Starting…");
  assert.equal(scanLine({ phase: 0, done: 0, total: 0, label: "" }), "Listing folders…");
  assert.equal(scanLine({ phase: 0, done: 1, total: 0, label: "" }), "Listing folders: 1 audio file so far");
  assert.equal(scanLine({ phase: 1, done: 40, total: 200, label: "" }), "Comparing files: 40 of 200");
  assert.equal(scanPercent({ phase: 0, done: 5, total: 0, label: "" }), 0);
  assert.equal(scanPercent({ phase: 1, done: 50, total: 200, label: "" }), 25);
  assert.equal(scanPercent({ phase: 1, done: 999, total: 200, label: "" }), 100);
  assert.equal(scanPercent(null), 0);
});

test("the card's Music folder keeps the separator the drive path uses", () => {
  assert.equal(cardMusicFolder("E:\\"), "E:\\.Pacemaker\\Music");
  assert.equal(cardMusicFolder("E:\\x"), "E:\\x\\.Pacemaker\\Music");
  assert.equal(cardMusicFolder("/media/pm/"), "/media/pm/.Pacemaker/Music");
  assert.equal(cardMusicFolder("/media/pm"), "/media/pm/.Pacemaker/Music");
});

test("rows with a match come first, each group in library order", () => {
  const rows = [
    { track_id: 9, candidates: [] },
    { track_id: 15, candidates: [1] },
    { track_id: 7, candidates: [] },
    { track_id: 3, candidates: [1, 2] },
  ];
  assert.deepEqual(sortRows(rows).map((r) => r.track_id), [3, 15, 7, 9]);
  assert.deepEqual(rows.map((r) => r.track_id), [9, 15, 7, 3], "the input is not reordered");
});

test("the Tick buttons only add ticks, so a hand-picked or hand-ticked row is never unticked by them", () => {
  const sure = { confidence: "sure", kind: "exact" };
  const likely = { confidence: "likely", kind: "same_name" };
  const manual = { confidence: "check", kind: "manual" };
  assert.equal(tickAfterBulk(true, manual, "sure"), true, "a hand pick stays ticked");
  assert.equal(tickAfterBulk(true, manual, "likely"), true);
  assert.equal(tickAfterBulk(true, likely, "sure"), true, "ticked by hand: kept");
  assert.equal(tickAfterBulk(false, likely, "sure"), false);
  assert.equal(tickAfterBulk(false, likely, "likely"), true);
  assert.equal(tickAfterBulk(false, sure, "sure"), true);
  assert.equal(tickAfterBulk(false, manual, "likely"), false, "a hand pick is never ticked by a bulk button");
  assert.equal(tickAfterBulk(true, sure, "none"), false, "Untick all clears every row, hand picks too");
  assert.equal(tickAfterBulk(true, manual, "none"), false);
  assert.equal(tickAfterBulk(true, undefined, "sure"), false, "a row without a file has nothing to tick");
});

test("the paths the app did not generate are said once, with the count", () => {
  assert.equal(nonStandardNote([{ non_standard: false }, {}]), "");
  assert.match(nonStandardNote([{ non_standard: true }, { non_standard: false }]), /^1 track will point at a path under Music that this app did not generate\..*not been checked on hardware/);
  assert.match(nonStandardNote([{ non_standard: true }, { non_standard: true }]), /^2 tracks will point/);
});
