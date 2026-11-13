// Run: node --experimental-strip-types --test src/lib/views/space/spaceLogic.test.mjs
import assert from "node:assert/strict";
import { test } from "node:test";
import {
  afterLine, bytesText, clampGb, clampInt, duplicateConflicts, filterItems, goalLine, hiddenLine, isOn, keepTicks, lastPlayedText,
  meterParts, needsAcknowledge, plural, runButton, runFlags, runLine, runTitle, sortItems, suggestedIds, tickTotals, toggled, verdict,
} from "./spaceLogic.ts";

const MB = 2 ** 20;
const GB = 2 ** 30;
const NOW = 1_800_000_000_000;
const DAY = 86_400_000;

const item = (id, over = {}) => ({
  track_id: id, label: `Example Artist ${id} - Demo Track ${id}`, format: "MP3", bytes: 100 * MB, play_time_secs: 200, rating: 0,
  times_played: 0, last_played_ms: null, added_ms: null, in_cases: [], in_mixes: [], reasons: [{ kind: "never_played", text: "Never played" }],
  duplicate_of: null, duplicate_of_label: null, blocked: null, suggested: false, ...over,
});

const plan = (over = {}) => ({
  rules_on: true, total_tracks: 100, need_bytes: 250 * MB, free_bytes: 2 * GB, total_bytes: 60 * GB, library_bytes: 50 * GB, usable_bytes: null,
  items: [], candidate_bytes: 0, removable_bytes: 1000 * MB, suggested_count: 0, suggested_bytes: 0, reachable: true,
  hidden: { favourites: 0, high_rated: 0, new_tracks: 0 }, missing_audio: 0, unchecked: 0, warnings: [], ...over,
});

test("sizes read like the Storage card", () => {
  assert.equal(bytesText(0), "0 B");
  assert.equal(bytesText(1536), "1.5 KB");
  assert.equal(bytesText(5 * MB), "5.0 MB");
  assert.equal(bytesText(2.5 * GB), "2.5 GB");
  assert.equal(plural(1, "track"), "1 track");
  assert.equal(plural(2, "track"), "2 tracks");
});

test("last played reads naturally and never goes negative", () => {
  assert.equal(lastPlayedText(null, NOW), "never");
  assert.equal(lastPlayedText(NOW - 1000, NOW), "today");
  assert.equal(lastPlayedText(NOW + 5000, NOW), "today", "a clock a little ahead is not negative");
  assert.equal(lastPlayedText(NOW - DAY, NOW), "1 day ago");
  assert.equal(lastPlayedText(NOW - 20 * DAY, NOW), "20 days ago");
  assert.equal(lastPlayedText(NOW - 200 * DAY, NOW), "6 months ago");
  assert.equal(lastPlayedText(NOW - 800 * DAY, NOW), "2 years ago");
});

test("ticked totals split what can be removed from what a mix holds", () => {
  const items = [item(1), item(2, { bytes: 50 * MB }), item(3, { blocked: "Used by the mix Demo Mix 1", bytes: 30 * MB }), item(4)];
  const t = tickTotals(items, new Set([2, 3, 99]));
  assert.deepEqual(t, { count: 2, bytes: 80 * MB, removeCount: 1, removeBytes: 50 * MB, blockedCount: 1 });
  assert.deepEqual(tickTotals(items, new Set()), { count: 0, bytes: 0, removeCount: 0, removeBytes: 0, blockedCount: 0 });
});

test("the suggested ticks are the planner's, and a re-plan keeps only ticks that still exist", () => {
  const items = [item(1, { suggested: true }), item(2), item(3, { suggested: true })];
  assert.deepEqual([...suggestedIds(items)], [1, 3]);
  assert.deepEqual([...keepTicks(new Set([2, 3, 7]), items)], [2, 3]);
});

test("the verdict says whether the ticks reach the target, in bytes", () => {
  const p = plan();
  const none = tickTotals([], new Set());
  assert.deepEqual(verdict(p, none), { tone: "warn", text: "250.0 MB more to tick to reach the target." });
  const enough = { ...none, removeBytes: 300 * MB };
  assert.deepEqual(verdict(p, enough), { tone: "ok", text: "Enough: removing the ticked tracks reaches the target (50.0 MB to spare)." });
  assert.equal(verdict(plan({ need_bytes: 0 }), none).tone, "ok");
  assert.equal(verdict(plan({ rules_on: false }), none).tone, "warn");
  const bad = verdict(plan({ reachable: false, removable_bytes: 100 * MB }), none);
  assert.equal(bad.tone, "bad");
  assert.match(bad.text, /100\.0 MB of removable candidates still leave 150\.0 MB short/);
  // a track a mix holds is archived but frees nothing, so it does not count toward the target
  const items = [item(1, { blocked: "Used by the mix Demo Mix 1", bytes: 400 * MB })];
  assert.equal(verdict(p, tickTotals(items, new Set([1]))).tone, "warn");
});

test("the goal and the after line describe both kinds of target", () => {
  const free = plan();
  assert.equal(goalLine(free, "free"), "2.0 GB free of 60.0 GB. 250.0 MB more must be freed.");
  assert.equal(goalLine(plan({ need_bytes: 0 }), "free"), "2.0 GB free of 60.0 GB. The target is met.");
  assert.equal(afterLine(free, "free", 500 * MB), "Free after removal: 2.5 GB");
  const card = plan({ usable_bytes: 40 * GB, need_bytes: 10 * GB });
  assert.equal(goalLine(card, "card"), "The library takes 50.0 GB; the card allows 40.0 GB. 10.0 GB must go.");
  assert.equal(goalLine(plan({ usable_bytes: 80 * GB, need_bytes: 0 }), "card"), "The library takes 50.0 GB; the card allows 80.0 GB. It already fits.");
  assert.equal(afterLine(card, "card", 12 * GB), "Library after removal: 38.0 GB");
  assert.equal(afterLine(card, "card", 90 * GB), "Library after removal: 0 B");
});

test("protected tracks are counted in words", () => {
  assert.equal(hiddenLine({ favourites: 0, high_rated: 0, new_tracks: 0 }), "");
  assert.equal(hiddenLine({ favourites: 1, high_rated: 0, new_tracks: 0 }), "1 track matched your rules but is protected: 1 in Favourites.");
  assert.equal(
    hiddenLine({ favourites: 2, high_rated: 3, new_tracks: 1 }),
    "6 tracks matched your rules but are protected: 2 in Favourites, 3 rated highly, 1 added recently.",
  );
});

test("ticking a copy and the track it repeats is flagged, with names", () => {
  const items = [
    item(1, { label: "Keeper" }),
    item(2, { label: "Copy A", duplicate_of: 1, duplicate_of_label: "Keeper" }),
    item(3, { label: "Copy B", duplicate_of: 1, duplicate_of_label: "Keeper" }),
    item(4, { label: "Lone copy", duplicate_of: 9, duplicate_of_label: "Not listed" }),
  ];
  assert.deepEqual(duplicateConflicts(items, new Set([2, 3])), [], "the original stays: nothing is lost");
  assert.deepEqual(duplicateConflicts(items, new Set([1, 2, 3])), [{ keeper: "Keeper", copies: ["Copy A", "Copy B"] }]);
  assert.deepEqual(duplicateConflicts(items, new Set([1])), []);
  assert.deepEqual(duplicateConflicts(items, new Set([4])), [], "its original is not even listed, so cannot be ticked here");
});

test("sorting by each column, never-played first as the oldest, ties by id", () => {
  const items = [
    item(1, { label: "b", bytes: 5, last_played_ms: NOW - DAY, rating: 3 }),
    item(2, { label: "A", bytes: 9, last_played_ms: null, rating: 1 }),
    item(3, { label: "c", bytes: 5, last_played_ms: NOW - 9 * DAY, rating: 1, reasons: [] }),
  ];
  const ids = (key, asc) => sortItems(items, key, asc).map((i) => i.track_id);
  assert.deepEqual(ids("label", true), [2, 1, 3], "case-insensitive");
  assert.deepEqual(ids("bytes", false), [2, 1, 3], "equal sizes keep id order");
  assert.deepEqual(ids("last_played", true), [2, 3, 1]);
  assert.deepEqual(ids("rating", true), [2, 3, 1]);
  assert.deepEqual(ids("reasons", true), [3, 1, 2]);
  assert.deepEqual(ids(null, true), [1, 2, 3], "no key: the planner's order");
  assert.deepEqual(items.map((i) => i.track_id), [1, 2, 3], "the input is not reordered");
});

test("the text filter looks at the name, the reasons and the cases", () => {
  const items = [
    item(1, { label: "Alpha - One" }),
    item(2, { label: "Beta - Two", reasons: [{ kind: "low_rating", text: "Rated 1 star" }] }),
    item(3, { label: "Gamma - Three", in_cases: ["Warm-up"] }),
  ];
  const ids = (q) => filterItems(items, q).map((i) => i.track_id);
  assert.deepEqual(ids(""), [1, 2, 3]);
  assert.deepEqual(ids("  "), [1, 2, 3]);
  assert.deepEqual(ids("beta"), [2]);
  assert.deepEqual(ids("rated 1"), [2]);
  assert.deepEqual(ids("warm"), [3]);
  assert.deepEqual(ids("zzz"), []);
});

const run = (over = {}) => ({
  items: [
    { track_id: 1, label: "One", bytes: 10 * MB, archive_name: "One.mp3", remove: true, note: null },
    { track_id: 2, label: "Two", bytes: 20 * MB, archive_name: "Two.mp3", remove: false, note: "stays on the card: the mix Demo Mix 1 uses it" },
  ],
  skipped: [], archive_bytes: 30 * MB, remove_bytes: 10 * MB, archive_free_bytes: null, problems: [], ...over,
});

test("the confirmation names the mode, the counts and what happens to each track", () => {
  const p = run();
  assert.deepEqual(runFlags("archive"), { archive: true, remove: false });
  assert.deepEqual(runFlags("remove"), { archive: false, remove: true });
  assert.deepEqual(runFlags("both"), { archive: true, remove: true });
  assert.equal(runTitle(p, "archive"), "Archive 2 tracks?");
  assert.equal(runTitle(p, "both"), "Archive 2 tracks and remove 1 from the card?");
  assert.equal(runButton(p, "archive"), "Archive 2 tracks (30.0 MB)");
  assert.equal(runButton(p, "both"), "Archive 2 and remove 1");
  assert.equal(runLine(p.items[0]), "archived as One.mp3; removed from the card");
  assert.equal(runLine(p.items[1]), "archived as Two.mp3; stays on the card: the mix Demo Mix 1 uses it");
  assert.equal(runLine({ ...p.items[0], grid_name: "One.str" }), "archived as One.mp3 with beat grid One.str; removed from the card");
  const removeOnly = run({ items: [{ track_id: 1, label: "One", bytes: 10 * MB, archive_name: null, remove: true, note: null }] });
  assert.equal(runTitle(removeOnly, "remove"), "Remove 1 track from the card?");
  assert.equal(runButton(removeOnly, "remove"), "Remove 1 track (10.0 MB)");
  assert.equal(runLine(removeOnly.items[0]), "removed from the card");
});

test("only a removal with no archive copy asks the user to acknowledge it", () => {
  const removeOnly = run({ items: [{ track_id: 1, label: "One", bytes: 1, archive_name: null, remove: true, note: null }] });
  assert.equal(needsAcknowledge(removeOnly, "remove"), true);
  assert.equal(needsAcknowledge(run(), "both"), false, "an archive copy exists");
  assert.equal(needsAcknowledge(run(), "archive"), false);
  const nothingRemoved = run({ items: [{ track_id: 2, label: "Two", bytes: 1, archive_name: null, remove: false, note: "stays" }] });
  assert.equal(needsAcknowledge(nothingRemoved, "remove"), false);
});

test("a 0-means-off setting becomes a checkbox and back without losing its value", () => {
  assert.equal(isOn(0), false);
  assert.equal(isOn(6), true);
  assert.equal(toggled(0, true, 6), 6, "switched on from off: the default");
  assert.equal(toggled(9, true, 6), 9, "already on: kept");
  assert.equal(toggled(9, false, 6), 0);
});

test("typed numbers are kept in range and junk keeps the old value", () => {
  assert.equal(clampInt("12", 0, 240, 6), 12);
  assert.equal(clampInt("9999", 0, 240, 6), 240);
  assert.equal(clampInt("-4", 0, 240, 6), 0);
  assert.equal(clampInt("", 0, 240, 6), 6);
  assert.equal(clampInt("abc", 0, 240, 6), 6);
  assert.equal(clampInt(7.9, 0, 10, 1), 7);
  assert.equal(clampGb("10.26", 0, 1000, 5), 10.3);
  assert.equal(clampGb("-2", 0, 1000, 5), 0);
  assert.equal(clampGb("1e9", 0, 1000, 5), 1000);
  assert.equal(clampGb("", 0, 1000, 5), 5);
});

test("the meter splits the card into used, freed by the ticks, free and over the limit", () => {
  const free = meterParts(plan({ total_bytes: 10 * GB, free_bytes: 2 * GB }), "free", 3 * GB);
  assert.deepEqual(free.parts, [{ key: "used", bytes: 5 * GB }, { key: "freed", bytes: 3 * GB }, { key: "free", bytes: 2 * GB }]);
  assert.equal(free.total, 10 * GB);
  assert.equal(free.label, "5.0 GB used, 3.0 GB ticked for removal, 2.0 GB free");
  // ticks cannot free more than is used
  assert.equal(meterParts(plan({ total_bytes: 10 * GB, free_bytes: 9 * GB }), "free", 5 * GB).parts[0].key, "freed");
  assert.deepEqual(meterParts(plan({ total_bytes: 0 }), "free", 0), { parts: [], total: 0, label: "Free space unknown" });

  // a 40 GB limit for a 50 GB library: 10 GB over; ticking 4 GB leaves 6 GB over
  const card = meterParts(plan({ library_bytes: 50 * GB, usable_bytes: 40 * GB }), "card", 4 * GB);
  assert.deepEqual(card.parts, [{ key: "used", bytes: 40 * GB }, { key: "over", bytes: 6 * GB }, { key: "freed", bytes: 4 * GB }]);
  // ticking enough leaves room
  const roomy = meterParts(plan({ library_bytes: 50 * GB, usable_bytes: 40 * GB }), "card", 15 * GB);
  assert.deepEqual(roomy.parts, [{ key: "used", bytes: 35 * GB }, { key: "freed", bytes: 15 * GB }, { key: "free", bytes: 5 * GB }]);
});
