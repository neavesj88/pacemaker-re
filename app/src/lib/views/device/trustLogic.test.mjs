// Run: node --experimental-strip-types --test src/lib/views/device/trustLogic.test.mjs
import assert from "node:assert/strict";
import { test } from "node:test";
import {
  ageText, deadRowLine, diffSentences, diffSummary, massWarning, moreText, pillText, planLine, removalReport, stampLabel, stampToDate,
  statusCounts, verifyVerdict, visibleChecks,
} from "./trustLogic.ts";

const zero = {
  tracks_returning: 0, tracks_lost: 0, tracks_changed: 0, cases_returning: 0, cases_lost: 0, cases_changed: 0,
  mixes_returning: 0, mixes_lost: 0, mixes_changed: 0,
};

test("snapshot stamps become fixed, locale-free text", () => {
  assert.equal(stampLabel("20261111T094512Z"), "11 Nov 2026, 09:45 UTC");
  assert.equal(stampLabel("20260102T235959Z"), "2 Jan 2026, 23:59 UTC");
  assert.equal(stampToDate("20261111T094512Z")?.toISOString(), "2026-11-11T09:45:12.000Z");
});

test("a stamp that is not a real date is shown as it is, not guessed", () => {
  for (const bad of ["", "nope", "20261311T000000Z", "20260231T000000Z", "20261111T250000Z", "20261111T0945Z"]) {
    assert.equal(stampToDate(bad), null, bad);
  }
  assert.equal(stampLabel("nope"), "nope");
  assert.equal(stampLabel(""), "unknown time");
});

test("ages read naturally and never go negative", () => {
  const now = 10_000_000_000;
  assert.equal(ageText(now - 5_000, now), "just now");
  assert.equal(ageText(now + 5_000, now), "just now", "a clock that is a little ahead");
  assert.equal(ageText(now - 60_000, now), "1 minute ago");
  assert.equal(ageText(now - 25 * 60_000, now), "25 minutes ago");
  assert.equal(ageText(now - 3 * 3_600_000, now), "3 hours ago");
  assert.equal(ageText(now - 3 * 86_400_000, now), "3 days ago");
});

test("a restore is described by what it does to the library", () => {
  assert.deepEqual(diffSentences(zero), []);
  assert.equal(diffSummary(zero), "Same as the card now");
  assert.equal(diffSummary(null), "");
  const c = { ...zero, tracks_lost: 1, tracks_returning: 2, tracks_changed: 3, mixes_lost: 1, cases_returning: 1 };
  const s = diffSentences(c);
  assert.equal(s[0], "1 track added since will leave the library (their files stay on the card)");
  assert.equal(s[1], "2 tracks will come back");
  assert.equal(s[2], "3 tracks will get their earlier tags, rating or key back");
  assert.ok(s.includes("1 mix made since will go") && s.includes("1 case will come back"));
  assert.equal(diffSummary(c), `${s[0]}; ${s[1]}; and more`);
  assert.equal(diffSummary({ ...zero, mixes_returning: 2 }), "2 mixes will come back");
});

test("capped lists say how many are not shown", () => {
  assert.equal(moreText(300, 300), "");
  assert.equal(moreText(450, 300), "and 150 more not listed here");
});

test("verification counts and filters the per-file rows", () => {
  const files = [
    { path: "a", status: "ok" }, { path: "b", status: "differs" }, { path: "c", status: "ok" }, { path: "d", status: "missing" },
    { path: "e", status: "unsafe" },
  ];
  assert.deepEqual(statusCounts(files), { ok: 2, differs: 1, missing: 1, unreadable: 0, unsafe: 1, bad_line: 0 });
  assert.deepEqual(visibleChecks(files, true).map((f) => f.path), ["b", "d", "e"]);
  assert.equal(visibleChecks(files, false).length, 5);
});

test("the verdict names the count, the problems and the strays", () => {
  assert.equal(verifyVerdict({ listed: 1, ok: 1, files: [{ status: "ok" }], extra: [] }), "The file matches its checksum.");
  assert.equal(verifyVerdict({ listed: 4, ok: 4, files: [], extra: [] }), "All 4 files match their checksums.");
  assert.equal(
    verifyVerdict({ listed: 3, ok: 1, files: [{ status: "ok" }, { status: "differs" }, { status: "missing" }], extra: ["x.txt"] }),
    "2 of 3 files do not match. 1 file in the folder is not in the manifest.",
  );
  assert.equal(
    verifyVerdict({ listed: 2, ok: 1, files: [{ status: "ok" }, { status: "missing" }], extra: ["x", "y"] }),
    "1 of 2 files does not match. 2 files in the folder are not in the manifest.",
  );
});

test("a dead row names the cases it would also leave", () => {
  assert.equal(deadRowLine({ label: "A - B", in_cases: [], in_mixes: [] }), "A - B");
  assert.equal(deadRowLine({ label: "A - B", in_cases: ["Warm up", "Peak"], in_mixes: [] }), "A - B (also removed from Warm up, Peak)");
});

test("a mostly-dead library gets the half-mounted-card warning, a normal one gets none", () => {
  const plan = { mass: false, dead_rows: [1], kept_for_mixes: [], total_tracks: 10, guard_percent: 50 };
  assert.equal(massWarning(plan), "");
  const w = massWarning({ ...plan, mass: true, dead_rows: [1, 2, 3, 4, 5, 6], kept_for_mixes: [7] });
  assert.match(w, /^7 of 10 tracks look dead, more than 50% of the library\./);
  assert.match(w, /not fully mounted/);
});

test("undo wording", () => {
  assert.equal(pillText("short"), "short");
  const long = "x".repeat(200);
  assert.equal(pillText(long).length, 88);
  assert.ok(pillText(long).endsWith("…"));
  assert.equal(planLine({ what: "Rating", current: "3 stars", becomes: "unrated", skip: null }), "Rating: 3 stars → unrated");
  assert.equal(
    planLine({ what: "Genre", current: "Techno", becomes: "House", skip: "changed since this edit (now Techno); left alone" }),
    "Genre: changed since this edit (now Techno); left alone",
  );
});

test("a removal is reported with what stayed and how to get it back", () => {
  assert.equal(
    removalReport({ removed: ["a", "b"], skipped: [], cases_touched: 1 }),
    "2 rows removed from the library and 1 case updated. A copy of music.db was saved first; Restore from a snapshot brings them back.",
  );
  const r = removalReport({ removed: ["a"], skipped: ["X: kept because the mix M uses it"], cases_touched: 0 });
  assert.match(r, /^1 row removed from the library\. A copy/);
  assert.match(r, /1 row left alone: X: kept because the mix M uses it\.$/);
});
