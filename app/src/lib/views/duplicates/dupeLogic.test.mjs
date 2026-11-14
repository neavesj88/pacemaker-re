// Run: node --experimental-strip-types --test src/lib/views/duplicates/dupeLogic.test.mjs
// The comparison table, reasons, choices, request and sentences of the duplicate finder.
// Synthetic names only.
import assert from "node:assert/strict";
import { test } from "node:test";
import { FIELDS, compareMembers, copiesOf, defaultChoices, fmtBytes, fmtDate, fmtLength, mergeButtonLabel, mergeable, pickedCount, planSentences, reasonLines, requestFor, totalsLine, waysLine, withExcluded, withKeeper } from "../../state/dupeLogic.ts";

const member = (id, over = {}) => ({
  track_id: id, title: "Sample Title", artist: "Sample Artist", album_artist: "", album: "", composer: "", genre: "House", label: "", key: "8A", comments: "", producer: "", remixer: "",
  year: 0, track_number: 0, disc_number: 0, format: "MP3", bit_rate_kbps: 320, sample_rate: 44100, file_size: 8_000_000, play_time_secs: 200, bpm: 128, rating: 0, times_played: 0,
  last_played: -1, date_added: 1_700_000_000_000, location: `/pmdb_tracks/${id}.mp3`, analysed: true, cases: [], mixes: [], file: "present", ...over,
});
const group = (members, links = [], keeper = members[0].track_id) => ({ id: Math.min(...members.map((m) => m.track_id)), members, links, modes: [...new Set(links.map((l) => l.mode))], suggested_keeper: keeper });
const link = (a, b, mode, score = null) => ({ a, b, mode, score, offset_secs: score === null ? null : 0 });
const settings = { by_tags: true, by_size: false, by_audio: true };

test("sizes, dates and lengths read plainly and the same everywhere", () => {
  assert.equal(fmtBytes(999), "999 B");
  assert.equal(fmtBytes(2048), "2 KB");
  assert.equal(fmtBytes(6_500_000), "6.2 MB");
  assert.equal(fmtDate(-1), "never");
  assert.equal(fmtDate(0, "-"), "-");
  // local time, like the Tracks columns: just after local midnight is still that day
  assert.equal(fmtDate(new Date(2026, 0, 1, 0, 30).getTime()), "2026-01-01");
  assert.equal(fmtDate(new Date(2026, 0, 1, 23, 30).getTime()), "2026-01-01");
  assert.equal(fmtLength(0), "–");
  assert.equal(fmtLength(65), "1:05");
  assert.equal(fmtLength(4325), "1:12:05", "hours, like the Tracks table");
  assert.equal(fmtLength(200.7), "3:20");
});

test("the comparison puts differing fields first and folds the rest, losing none", () => {
  const a = member(1, { bit_rate_kbps: 128, cases: [{ case_id: 1, name: "Sample Case", position: 2, of: 5 }] });
  const b = member(2, { artist: "", mixes: ["Sample Mix"], file: "missing", last_played: 1_700_000_000_000 });
  const { differ, same } = compareMembers([a, b]);
  const keys = differ.map((d) => d.key);
  assert.deepEqual(keys, ["artist", "bitrate", "last_played", "cases", "mixes", "file", "location"]);
  assert.deepEqual(differ.find((d) => d.key === "bitrate").values, ["128 kbps", "320 kbps"]);
  assert.deepEqual(differ.find((d) => d.key === "artist").values, ["Sample Artist", "–"]);
  assert.deepEqual(differ.find((d) => d.key === "cases").values, ["Sample Case (2 of 5)", "none"]);
  assert.deepEqual(differ.find((d) => d.key === "file").values, ["present", "missing"]);
  assert.equal(same.find((s) => s.label === "Title").value, "Sample Title");
  assert.equal(differ.length + same.length, FIELDS.length, "every field is either shown or folded");
  // identical tracks differ only in where they live
  const twin = compareMembers([member(1), member(2)]);
  assert.deepEqual(twin.differ.map((d) => d.key), ["location"]);
});

test("the reasons say which ways found the group and how alike the sound is", () => {
  assert.deepEqual(reasonLines({ links: [link(1, 2, "tags")] }), ["same artist and title"]);
  assert.deepEqual(reasonLines({ links: [link(1, 2, "tags"), link(2, 3, "size"), link(1, 3, "audio", 0.9712)] }), ["same artist and title", "same file size and length", "sound 97% alike"]);
  assert.deepEqual(reasonLines({ links: [link(1, 2, "audio", 0.93), link(1, 3, "audio", 0.99999)] }), ["sound 93%–99% alike"], "never rounds up to 100%");
});

test("default choices keep the suggestion and pick every group", () => {
  const g1 = group([member(1), member(2)], [link(1, 2, "tags")], 2);
  const g2 = group([member(5), member(6), member(7)], [link(5, 6, "tags"), link(6, 7, "tags")], 6);
  const result = { groups: [g1, g2], pending: [], fingerprinted: 0, tracks: 7, warnings: [] };
  const c = defaultChoices(result);
  assert.deepEqual(c[1], { keeper: 2, excluded: [], picked: true });
  assert.deepEqual(copiesOf(g2, c[5]), [5, 7]);
  assert.equal(pickedCount(result, c), 2);
  assert.deepEqual(defaultChoices(null), {});
});

test("changing the keeper or marking a non-copy keeps the choice consistent", () => {
  const g = group([member(1), member(2), member(3)], [link(1, 2, "tags"), link(2, 3, "tags")], 1);
  let c = defaultChoices({ groups: [g] })[1];
  c = withExcluded(c, 3, true);
  assert.deepEqual(copiesOf(g, c), [2]);
  assert.deepEqual(withExcluded(c, 1, true).excluded, [3], "the keeper cannot be marked as not a copy");
  c = withKeeper(c, 3);
  assert.equal(c.keeper, 3);
  assert.deepEqual(c.excluded, [], "the new keeper is no longer excluded");
  assert.deepEqual(copiesOf(g, c), [1, 2]);
  c = withExcluded(withExcluded(c, 1, true), 2, true);
  assert.equal(mergeable(g, c), false, "no copy left");
  assert.equal(mergeable(g, undefined), false);
  assert.equal(mergeable(g, { keeper: 99, excluded: [], picked: true }), false, "a keeper outside the group");
  assert.deepEqual(withExcluded(c, 1, false).excluded, [2]);
});

test("the request holds only picked, mergeable groups and ties deleting files to removing rows", () => {
  const g1 = group([member(1), member(2)], [link(1, 2, "tags")], 1);
  const g2 = group([member(5), member(6)], [link(5, 6, "tags")], 6);
  const g3 = group([member(8), member(9)], [link(8, 9, "tags")], 8);
  const result = { groups: [g1, g2, g3], pending: [], fingerprinted: 0, tracks: 9, warnings: [] };
  const choices = defaultChoices(result);
  choices[5].picked = false;
  choices[8] = withExcluded(choices[8], 9, true);
  const r = requestFor(result, choices, true, true, settings);
  assert.deepEqual(r.groups, [{ keeper: 1, others: [2] }]);
  assert.deepEqual([r.remove_rows, r.delete_files], [true, true]);
  assert.equal(r.settings, settings);
  assert.equal(requestFor(result, choices, false, true, settings).delete_files, false, "files are never deleted unless the rows are removed");
  assert.equal(pickedCount(result, choices), 1);
});

const plan = () => ({
  groups: [
    {
      keeper: 1,
      others: [2, 3],
      case_edits: [
        { case_id: 10, case_name: "Sample Case", kind: "replace", others: [2, 3] },
        { case_id: 11, case_name: "Other Case", kind: "already_there", others: [2] },
      ],
      stats: { rating: { before: 2, after: 4 }, times_played: { before: 3, after: 9 }, last_played: { before: -1, after: 1_700_000_000_000 } },
      remove: [{ track_id: 2, location: "/pmdb_tracks/a/b/c.mp3" }],
      blocked: [{ track_id: 3, mixes: ["Sample Mix"] }],
    },
  ],
  labels: [{ track_id: 1, label: "Sample Artist - Sample Title" }, { track_id: 2, label: "Sample Artist - Sample Title (copy)" }],
  files: [
    { track_id: 2, path: "a/b/c.mp3", kind: "audio", size: 6_500_000, exists: true, kept_because: null },
    { track_id: 2, path: "a/b/c.str", kind: "grid", size: 2048, exists: true, kept_because: null },
  ],
  notes: [],
  totals: { groups: 1, case_entries: 3, rows_removed: 1, files_deleted: 2, bytes: 6_502_048 },
  digest: "abc",
});

test("the plan reads out every change to every copy", () => {
  const [g] = planSentences(plan());
  assert.equal(g.keeper, "Sample Artist - Sample Title (#1)");
  assert.equal(g.copies[0].label, "Sample Artist - Sample Title (copy) (#2)", "two copies with one title stay tellable apart by id");
  assert.equal(g.stats, `The kept track gets: rating 2 → 4, plays 3 → 9, last played never → ${fmtDate(1_700_000_000_000)}.`);
  assert.deepEqual(g.copies[0].parts, [
    "its place in “Sample Case” goes to the kept track",
    "leaves “Other Case” (the kept track is there already)",
    "removed from the library",
    "deletes audio file a/b/c.mp3 (6.2 MB)",
    "deletes beat grid file a/b/c.str (2 KB)",
  ]);
  assert.equal(g.copies[1].label, "track 3", "an unlabelled id is still named");
  assert.deepEqual(g.copies[1].parts, ["its place in “Sample Case” goes to the kept track", "stays in the library: the mix “Sample Mix” uses it"]);
});

test("a file another track uses and a file already gone are said so, and merge-only says nothing is removed", () => {
  const p = plan();
  p.files[0].kept_because = "another track in the library uses this file";
  p.files[1].exists = false;
  const parts = planSentences(p)[0].copies[0].parts;
  assert.ok(parts.includes("keeps audio file a/b/c.mp3 (another track in the library uses this file)"));
  assert.ok(parts.includes("beat grid file a/b/c.str is missing"));
  const mergeOnly = plan();
  mergeOnly.groups[0].remove = [];
  mergeOnly.groups[0].blocked = [];
  mergeOnly.files = [];
  mergeOnly.groups[0].stats = { rating: { before: 1, after: 1 }, times_played: { before: 0, after: 0 }, last_played: { before: -1, after: -1 } };
  const [g] = planSentences(mergeOnly);
  assert.equal(g.stats, null);
  assert.ok(g.copies[0].parts.includes("stays in the library"));
});

test("the totals and the button say what will happen to files", () => {
  const p = plan();
  assert.equal(totalsLine(p), "1 group · 3 case places moved · 1 copy removed from the library · 2 files deleted (6.2 MB)");
  assert.equal(mergeButtonLabel(p, true, true), "Merge 1 group and delete 2 files");
  assert.equal(mergeButtonLabel(p, true, false), "Merge 1 group and remove 1 copy");
  assert.equal(mergeButtonLabel(p, false, false), "Merge 1 group");
  assert.equal(mergeButtonLabel(null, true, true), "Merge");
  p.totals = { groups: 2, case_entries: 0, rows_removed: 0, files_deleted: 0, bytes: 0 };
  assert.equal(totalsLine(p), "2 groups · 0 case places moved");
});

test("the settings line names the ways that are on", () => {
  assert.equal(waysLine({ by_tags: true, by_size: false, by_audio: true }), "Looking by artist and title, sound");
  assert.equal(waysLine({ by_tags: false, by_size: false, by_audio: false }), "No way of finding copies is switched on");
});
