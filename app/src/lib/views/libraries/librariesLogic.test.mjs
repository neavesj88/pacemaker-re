// Run: node --experimental-strip-types --test src/lib/views/libraries/librariesLogic.test.mjs
import assert from "node:assert/strict";
import { test } from "node:test";
import { libraryOptions } from "../sync/syncLogic.ts";
import {
  buildSelection, canCopy, defaultCasePick, intoCaseLabel, MAX_CASE_NAME, NEW_CASE, resolveIntoCase, clickSelect, clock, copyLabel, copySummary, defaultRight, readsAtOnce, footerText, sentenceName, dialogTitle, libraryNameFor, dragFrom, dragIds, dragLabel, emptySelected, linesOf,
  num, onBoth, parseDragFrom, presenceOf, pruneTo, readButtonText, resolveDrop, rightOptions, rowsOf, selectionSize, statusText,
} from "./librariesLogic.ts";

const MB = 2 ** 20;
const COMP = "C:\\Users\\demo\\Music\\Pacemaker Revival";
const trk = (id, title, artist = "Example Artist", loc = `/pmdb_tracks/0/0/t${id}.mp3`) => ({
  track_id: id, title, artist, album: "Sample Album", genre: "House", key: "Am", label: "", bpm: 128, play_time_secs: 200, location: loc,
});
const col = (id, name, track_count = 3) => ({ id, name, play_time_secs: 600, track_count });
const lib = (root, tracks, mixes = [], cases = []) => ({ root, kind: root.length <= 3 ? "pacemaker" : "computer", tracks, mixes, cases });

const mine = lib(COMP, [trk(1, "Night Drive"), trk(2, "Glass Rooms", "Sample Band"), trk(3, "Slow Lights")], [col(1, "Friday warm-up"), col(2, "Road trip")], [col(1, "Opening"), col(2, "New finds")]);
const pm = lib("G:\\", [trk(10, "Night Drive", "Example Artist", "/pmdb_tracks/0/0/t1.mp3"), trk(11, "Harbour")], [col(5, " friday WARM-UP ")], [col(7, "Crate")]);

const item = (key, kind, action, over = {}) => ({
  key, kind, action, label: key, bytes: 0, needs: [], detail: "", global_id: null, location: null, source_id: 1, target_id: null, blockers: [],
  needed_by: [], into_case: null, ...over,
});
const plan = (items, over = {}) => ({
  items, settings: { tracks_on_both: "newer", mixes_same_name: "newer", cases_same_name: "merge", remove_missing: false, play_counts: "higher" },
  source_label: "computer", bytes_to_copy: items.filter((i) => i.kind === "track" && i.action.type !== "skip").reduce((n, i) => n + i.bytes, 0),
  free_bytes: 100 * MB, fits: true, digest: "d", notes: [], ...over,
});

test("lengths read as m:ss or h:mm:ss", () => {
  assert.equal(clock(215), "3:35");
  assert.equal(clock(59), "0:59");
  assert.equal(clock(3725), "1:02:05");
  assert.equal(clock(0), "");
  assert.equal(clock(-3), "");
  assert.equal(clock(NaN), "");
});

test("a search needs every word, in a track's text or a name; empty gives the list back", () => {
  assert.equal(rowsOf(mine, "tracks", "").length, 3);
  assert.equal(rowsOf(mine, "tracks", "   ").length, 3);
  assert.deepEqual(rowsOf(mine, "tracks", "sample glass").map((t) => t.track_id), [2]);
  assert.deepEqual(rowsOf(mine, "tracks", "NIGHT").map((t) => t.track_id), [1]);
  assert.equal(rowsOf(mine, "tracks", "nope").length, 0);
  assert.deepEqual(rowsOf(mine, "mixes", "road").map((m) => m.id), [2]);
  assert.deepEqual(rowsOf(mine, "cases", "new fin").map((m) => m.id), [2]);
});

test("on both: a track by its place, a mix or case by its trimmed, case-folded name", () => {
  const there = presenceOf(pm);
  assert.equal(onBoth("tracks", mine.tracks[0], there), true);
  assert.equal(onBoth("tracks", mine.tracks[1], there), false, "same title elsewhere does not count, only the place");
  assert.equal(onBoth("mixes", mine.mixes[0], there), true);
  assert.equal(onBoth("mixes", mine.mixes[1], there), false);
  assert.equal(onBoth("cases", mine.cases[0], there), false);
  assert.equal(onBoth("tracks", mine.tracks[0], null), false, "nothing is marked until the other side is read");
  assert.equal(onBoth("tracks", mine.tracks[0], presenceOf(null)), false);
});

test("clicks: plain selects one, ctrl toggles, shift takes the range from the anchor", () => {
  const ids = [1, 2, 3, 4, 5];
  let s = clickSelect(new Set(), null, ids, 1, { shift: false, ctrl: false });
  assert.deepEqual([...s.selected], [2]);
  assert.equal(s.anchor, 2);
  s = clickSelect(s.selected, s.anchor, ids, 3, { shift: true, ctrl: false });
  assert.deepEqual([...s.selected], [2, 3, 4]);
  assert.equal(s.anchor, 2, "the anchor stays for the next shift-click");
  s = clickSelect(s.selected, s.anchor, ids, 0, { shift: true, ctrl: false });
  assert.deepEqual([...s.selected].sort(), [1, 2], "the range follows the anchor, not the old range");
  s = clickSelect(s.selected, s.anchor, ids, 4, { shift: false, ctrl: true });
  assert.deepEqual([...s.selected].sort(), [1, 2, 5]);
  s = clickSelect(s.selected, s.anchor, ids, 4, { shift: false, ctrl: true });
  assert.deepEqual([...s.selected].sort(), [1, 2], "ctrl on a selected row takes it out");
  s = clickSelect(new Set([1]), 1, ids, 4, { shift: true, ctrl: true });
  assert.deepEqual([...s.selected].sort(), [1, 2, 3, 4, 5], "ctrl+shift adds the range");
  s = clickSelect(new Set(), null, ids, 2, { shift: true, ctrl: false });
  assert.deepEqual([...s.selected], [3], "shift without an anchor selects the row");
  assert.deepEqual([...clickSelect(new Set([1]), 1, ids, 99, { shift: false, ctrl: false }).selected], [1], "a click past the end changes nothing");
});

test("a search takes out the selected rows it hides", () => {
  assert.deepEqual([...pruneTo(new Set([1, 2, 3]), [2, 3, 9])].sort(), [2, 3]);
  assert.equal(pruneTo(new Set(), [1]).size, 0);
});

test("a drag carries the selection when it starts on a selected row, else just the pressed row", () => {
  const visible = [5, 4, 3, 2, 1];
  assert.deepEqual(dragIds(new Set([2, 4]), 2, visible), [4, 2], "in list order");
  assert.deepEqual(dragIds(new Set([2, 4]), 3, visible), [3]);
  assert.deepEqual(dragIds(new Set(), 1, visible), [1]);
  assert.equal(dragLabel("tracks", 3), "3 tracks");
  assert.equal(dragLabel("mixes", 1), "1 mix");
  assert.equal(dragLabel("mixes", 2), "2 mixes");
  assert.equal(dragLabel("cases", 1), "1 case");
});

test("the selection sent to the backend: locations, mix names or case names, in library order", () => {
  assert.deepEqual(buildSelection(mine, "tracks", [3, 1]), { tracks: [mine.tracks[0].location, mine.tracks[2].location], mixes: [], cases: [], into_case: null });
  assert.deepEqual(buildSelection(mine, "mixes", [2]), { tracks: [], mixes: ["Road trip"], cases: [], into_case: null });
  assert.deepEqual(buildSelection(mine, "cases", [1, 2]).cases, ["Opening", "New finds"]);
  const into = buildSelection(mine, "tracks", [1], { name: "Crate", create: false });
  assert.deepEqual(into.into_case, { name: "Crate", create: false });
  assert.equal(buildSelection(mine, "mixes", [1], { name: "Crate", create: false }).into_case, null, "only tracks go into a case");
  assert.equal(buildSelection(mine, "tracks", [], { name: "Crate", create: false }).into_case, null);
  assert.equal(selectionSize(buildSelection(mine, "tracks", [1, 2, 3])), 3);
  assert.equal(selectionSize(buildSelection(mine, "tracks", [])), 0);
});

test("drop targets: the other pane copies, the same pane does nothing, a case row takes tracks into it", () => {
  const base = { fromSide: "left", fromTab: "tracks", overSide: "right", overTab: "tracks", overCase: null };
  assert.deepEqual(resolveDrop(base), { into: null });
  assert.equal(resolveDrop({ ...base, overSide: "left" }), null, "dropping back on the pane it came from does nothing");
  assert.equal(resolveDrop({ ...base, overSide: "left", overTab: "cases", overCase: "Crate" }), null, "not even onto one of its own case rows");
  assert.deepEqual(resolveDrop({ ...base, overTab: "cases", overCase: "Crate" }), { into: { name: "Crate", create: false } });
  assert.deepEqual(resolveDrop({ ...base, overTab: "cases", overCase: null }), { into: null }, "the empty part of the Cases tab is a plain copy");
  assert.deepEqual(resolveDrop({ ...base, overTab: "mixes", overCase: "Crate" }), { into: null }, "a case row only counts in the Cases tab");
  assert.deepEqual(resolveDrop({ ...base, fromTab: "mixes", overTab: "cases", overCase: "Crate" }), { into: null }, "mixes and cases never go into a case");
  assert.deepEqual(resolveDrop({ ...base, fromSide: "right", overSide: "left", fromTab: "cases" }), { into: null });
});

test("drag names round-trip and nothing else is taken for a pane drag", () => {
  assert.equal(dragFrom("left", "mixes"), "lib:left:mixes");
  assert.deepEqual(parseDragFrom("lib:right:cases"), { side: "right", tab: "cases" });
  assert.equal(parseDragFrom("tracks"), null);
  assert.equal(parseDragFrom("case:7"), null);
  assert.equal(parseDragFrom("mix:picker"), null);
  assert.equal(parseDragFrom("lib:up:tracks"), null);
});

test("the right pane: not the open library, defaulting to the other kind, never reading a Pacemaker on its own", () => {
  const o = libraryOptions({ path: COMP, exists: true }, ["G:\\", "H:\\"], { root: COMP, kind: "computer" });
  const open = { root: COMP, kind: "computer" };
  assert.deepEqual(rightOptions(o, open).map((x) => x.label), ["Pacemaker (G:)", "Pacemaker (H:)"]);
  assert.equal(defaultRight(o, open, ""), "G:\\", "a computer library is open: the detected Pacemaker");
  assert.equal(defaultRight(o, open, "H:\\"), "H:\\", "a choice that is still on offer is kept");
  assert.equal(defaultRight(o, open, "Z:\\"), "G:\\", "an unplugged one is replaced");
  assert.equal(defaultRight(o, open, COMP), "G:\\", "never the open library itself");
  const pmOpen = { root: "G:\\", kind: "pacemaker" };
  const o2 = libraryOptions({ path: COMP, exists: true }, ["G:\\"], pmOpen);
  assert.equal(defaultRight(o2, pmOpen, ""), COMP, "a Pacemaker is open: the library on this computer");
  const o3 = libraryOptions({ path: COMP, exists: false }, ["G:\\"], pmOpen);
  assert.equal(defaultRight(o3, pmOpen, ""), "", "nothing else to show");
  assert.equal(defaultRight(o, null, ""), COMP, "no library open: the library on this computer, never a Pacemaker");
  assert.equal(defaultRight(libraryOptions({ path: COMP, exists: false }, ["G:\\"], null), null, ""), "", "no library open and no computer library");
  // the never-read-a-Pacemaker-on-its-own rule: this is the function the store asks
  assert.equal(readsAtOnce("G:\\", ["G:\\"], [], false), false, "a detected Pacemaker waits for the button");
  assert.equal(readsAtOnce("g:/", ["G:\\"], [], false), false, "however its root is spelt");
  assert.equal(readsAtOnce("G:\\", ["G:\\"], [], true), true, "the button reads it");
  assert.equal(readsAtOnce(COMP, ["G:\\"], [], false), true, "this computer is read when chosen");
  assert.equal(readsAtOnce("G:\\", ["G:\\"], ["G:\\"], false), true, "a folder opened by hand is a deliberate choice");
  assert.equal(readsAtOnce("", [], [], true), false, "nothing chosen, nothing read");
  assert.equal(readButtonText("G:\\"), "Read the Pacemaker (G:)");
});

test("button and status words", () => {
  assert.equal(copyLabel("left", "Pacemaker (G:)"), "Copy to Pacemaker (G:) →");
  assert.equal(copyLabel("right", "This computer"), "← Copy to This computer");
  assert.equal(num(30421), "30,421");
  assert.equal(num(999), "999");
  assert.equal(statusText("tracks", 0, 30421, 30421), "30,421 tracks");
  assert.equal(statusText("tracks", 3, 120, 30421), "3 selected · 120 of 30,421 tracks");
  assert.equal(statusText("mixes", 1, 1, 1), "1 selected · 1 mix");
  assert.equal(statusText("cases", 0, 0, 0), "0 cases");
  assert.equal(emptySelected().tracks.size, 0);
});

test("the dialog names every item with what is done and why", () => {
  const p = plan([
    item("track:/t/1", "track", { type: "copy" }, { label: "Example Artist - Night Drive", bytes: 8 * MB, needed_by: ['mix "Friday warm-up"'] }),
    item("track:/t/2", "track", { type: "copy" }, { label: "Sample Band - Glass Rooms", bytes: 9 * MB }),
    item("track:/t/3", "track", { type: "skip", reason: "already the same" }, { label: "Slow Lights" }),
    item("track:/t/4", "track", { type: "skip", reason: "its audio file is missing" }, { label: "Lost Reel" }),
    item("track:/t/5", "track", { type: "update" }, { label: "Evening Train", bytes: 1000 }),
    item("mix:friday warm-up", "mix", { type: "copy" }, { label: "Friday warm-up" }),
    item("mix:set", "mix", { type: "copy_as_new", name: "Set (from computer)" }, { label: "Set" }),
    item("case:opening", "case", { type: "merge" }, { label: "Opening" }),
    item("into-case:crate", "case", { type: "merge" }, { label: "Add 2 tracks to case Crate", into_case: "Crate" }),
    item("into-case:fresh", "case", { type: "copy" }, { label: "New case Fresh with 2 tracks", into_case: "Fresh", detail: "1 of 3 tracks will not be there and are left out" }),
    item("into-case:done", "case", { type: "skip", reason: "already the same" }, { label: "Add 1 track to case Done", into_case: "Done" }),
  ]);
  const l = linesOf(p);
  const by = (k) => [...l.acting, ...l.skipped, ...l.same].find((x) => x.key === k);
  assert.equal(by("track:/t/1").action, "Copy");
  assert.equal(by("track:/t/1").why, 'Needed by mix "Friday warm-up"');
  assert.equal(by("track:/t/2").why, "", "a track you picked needs no reason");
  assert.deepEqual([by("track:/t/3").action, by("track:/t/3").why], ["Skip", "already there"]);
  assert.deepEqual([by("track:/t/4").action, by("track:/t/4").why], ["Skip", "its audio file is missing"]);
  assert.equal(by("track:/t/5").action, "Update");
  assert.match(by("mix:friday warm-up").why, /no mix with this name/);
  assert.equal(by("mix:set").action, 'Copy as "Set (from computer)"');
  assert.equal(by("case:opening").action, "Merge");
  assert.deepEqual([by("into-case:crate").action, by("into-case:crate").name], ["Add to case Crate", "2 tracks"]);
  assert.deepEqual([by("into-case:fresh").action, by("into-case:fresh").name], ["New case", "Fresh with 2 tracks"]);
  assert.equal(by("into-case:done").name, "Add 1 track to case Done", "a skipped one keeps its whole label");
  assert.match(by("into-case:fresh").why, /1 of 3 tracks/);
  assert.deepEqual([by("into-case:done").action, by("into-case:done").why], ["Skip", "already there"]);
  assert.deepEqual(l.acting.map((x) => x.key).slice(0, 3), ["track:/t/1", "track:/t/2", "track:/t/5"], "what happens comes first, in the plan's order");
  assert.equal(l.skipped.length, 1);
  assert.equal(l.same.length, 2);
  assert.equal(l.acting.length + l.skipped.length + l.same.length, p.items.length, "every item is named");
});

test("the summary sentence and whether the copy can run", () => {
  const copyOnly = plan([item("track:/t/1", "track", { type: "copy" }, { bytes: 3 * MB }), item("track:/t/2", "track", { type: "copy" }, { bytes: 3 * MB }), item("mix:a", "mix", { type: "copy" })]);
  assert.equal(copySummary(copyOnly), "Will copy 2 tracks, bring 1 mix.");
  assert.equal(canCopy(copyOnly), true);
  const into = plan([item("track:/t/1", "track", { type: "copy" }, { bytes: MB }), item("into-case:crate", "case", { type: "merge" }, { label: "Add 1 track to case Crate", into_case: "Crate" })]);
  assert.equal(copySummary(into), "Will copy 1 track, add 1 track to case Crate.");
  const fresh = plan([item("into-case:f", "case", { type: "copy" }, { label: "New case F with 2 tracks", into_case: "F" })]);
  assert.equal(copySummary(fresh), "Will make new case F with 2 tracks.");
  const plays = plan([item("track:/t/1", "track", { type: "update_plays" }), item("mix:a", "mix", { type: "update_plays" })]);
  assert.equal(copySummary(plays), "Will update the play counts of 2 items.");
  const rep = plan([item("track:/t/1", "track", { type: "repair" }, { bytes: MB, detail: "the audio file is missing on the target" })]);
  assert.equal(copySummary(rep), "Will copy the missing files of 1 track.");
  assert.equal(canCopy(rep), true);
  assert.deepEqual([linesOf(rep).acting[0].action, linesOf(rep).acting[0].why], ["Repair", "the audio file is missing on the target"]);
  assert.match(footerText(rep, "Pacemaker (G:)"), /^Nothing on Pacemaker \(G:\) is removed or overwritten\./, "a repair only adds files");
  const nothing = plan([item("track:/t/1", "track", { type: "skip", reason: "already the same" })]);
  assert.equal(copySummary(nothing), "Nothing will be copied: all of it is there already.");
  assert.equal(canCopy(nothing), false);
  const mixed = plan([item("track:/t/1", "track", { type: "skip", reason: "its analysis file (.str) is empty" }), item("track:/t/2", "track", { type: "skip", reason: "already the same" })]);
  assert.equal(copySummary(mixed), "Nothing will be copied.", "no 'already there' when a row is skipped for another reason");
  assert.equal(canCopy(plan([])), false);
  assert.equal(footerText(copyOnly, "This computer"), "Nothing on this computer is removed or overwritten. A copy of the database on this computer is saved before the first change.");
  assert.match(footerText(plays, "Pacemaker (G:)"), /Play counts only go up/);
  assert.match(footerText(plan([item("mix:a", "mix", { type: "replace" })]), "Pacemaker (G:)"), /changes mixes, cases or track details/, "never claims nothing changes when something would");
  assert.equal(sentenceName("This computer"), "this computer");
  const intoNames = plan([item("into-case:crate", "case", { type: "merge" }, { label: "Add 2 tracks to case Crate", into_case: "Crate", needs: [{ location: "/a", label: "A - One" }, { location: "/b", label: "B - Two" }] })]);
  assert.deepEqual(linesOf(intoNames).acting[0].names, ["A - One", "B - Two"], "the into-case row names its tracks");
  const big = plan([item("track:/t/1", "track", { type: "copy" }, { bytes: 50 * MB })], { free_bytes: 10 * MB, fits: false });
  assert.equal(canCopy(big), false, "does not fit");
  assert.equal(dialogTitle("Pacemaker (G:)"), "Copy to Pacemaker (G:)?");
  assert.equal(libraryNameFor("G:\\"), "Pacemaker (G:)");
  assert.equal(libraryNameFor(COMP), "This computer");
});

test("the click route into a case: the picker's choice becomes the same target a drop on a case row gives", () => {
  const cases = [{ id: 1, name: "Warm-up", track_count: 4, play_time_secs: 0 }, { id: 2, name: "Crate A", track_count: 0, play_time_secs: 0 }];
  assert.equal(intoCaseLabel("Pacemaker (G:)"), "Copy into a case on Pacemaker (G:)…");
  assert.equal(defaultCasePick(cases), "Warm-up");
  assert.equal(defaultCasePick([]), NEW_CASE, "no cases there: a new one");
  // an existing case: the library's own spelling, nothing created (as the drop on its row)
  assert.deepEqual(resolveIntoCase(cases, "Crate A", ""), { into: { name: "Crate A", create: false } });
  assert.deepEqual(resolveIntoCase(cases, "crate a", ""), { into: { name: "Crate A", create: false } });
  assert.deepEqual(resolveIntoCase(cases, "Gone", ""), { error: "That case is not on the other library any more. Pick another." });
  // a new case: the trimmed name; a name that is already a case there is that case, not a second one
  assert.deepEqual(resolveIntoCase(cases, NEW_CASE, "  Fresh finds "), { into: { name: "Fresh finds", create: true } });
  assert.deepEqual(resolveIntoCase(cases, NEW_CASE, " WARM-UP"), { into: { name: "Warm-up", create: false } });
  assert.deepEqual(resolveIntoCase(cases, NEW_CASE, "   "), { error: "Type a name for the new case." });
  assert.match(resolveIntoCase(cases, NEW_CASE, "x".repeat(MAX_CASE_NAME + 1)).error, /at most 255 characters/);
  assert.equal(resolveIntoCase(cases, NEW_CASE, "x".repeat(MAX_CASE_NAME)).into.create, true);
  // the selection it plans is the drop's: the picked tracks, and the case as the target
  const data = { root: "C:/x", kind: "computer", tracks: [{ track_id: 1, location: "/a" }, { track_id: 2, location: "/b" }], mixes: [], cases: [] };
  const into = resolveIntoCase(cases, "Warm-up", "").into;
  assert.deepEqual(buildSelection(data, "tracks", [2], into), buildSelection(data, "tracks", [2], resolveDrop({ fromSide: "left", fromTab: "tracks", overSide: "right", overTab: "cases", overCase: "Warm-up" }).into));
});
