// Run: node --experimental-strip-types --test src/lib/views/sync/syncLogic.test.mjs
import assert from "node:assert/strict";
import { test } from "node:test";
import {
  actionText, defaultPair, dependencyMarks, excludedKeys, groupItems, isStale, libraryOptions, markText, pairFor, pairReady, pickSide, removalList,
  resultParts, sizeText, spaceLine, swapPair, tally, tallyText,
} from "./syncLogic.ts";

const MB = 2 ** 20;
const GB = 2 ** 30;
const COMP = "C:\\Users\\demo\\Music\\Pacemaker Revival";
const opts = (vols = ["G:\\"], exists = true, open = null) => libraryOptions({ path: COMP, exists }, vols, open);

const item = (key, kind, action, over = {}) => ({
  key, kind, action, label: key, bytes: 0, needs: [], detail: "", global_id: null, location: null, source_id: 1, target_id: null, blockers: [], ...over,
});
const trk = (n, action = { type: "copy" }, bytes = MB) => item(`track:/t/${n}`, "track", action, { label: `Track ${n}`, bytes, location: `/t/${n}` });
const need = (n) => ({ location: `/t/${n}`, label: `Track ${n}` });
const mix = (name, needs) => item(`mix:${name}`, "mix", { type: "copy" }, { label: name, needs });
const cse = (name, needs) => item(`case:${name}`, "case", { type: "merge" }, { label: name, needs });

test("size formatting", () => {
  assert.equal(sizeText(80), "80 B");
  assert.equal(sizeText(2048), "2.0 KB");
  assert.equal(sizeText(5 * MB + MB / 2), "5.5 MB");
  assert.equal(sizeText(1.2 * GB), "1.2 GB");
});

test("libraries on offer: the computer one only when it is there, then each Pacemaker, then a folder opened by hand", () => {
  assert.deepEqual(opts().map((o) => o.label), ["This computer", "Pacemaker (G:)"]);
  assert.deepEqual(opts(["G:\\"], false).map((o) => o.label), ["Pacemaker (G:)"]);
  assert.deepEqual(opts([], false, { root: COMP, kind: "computer" }).map((o) => o.label), ["This computer"]);
  const hand = opts(["G:\\"], true, { root: "E:\\cards\\mine", kind: "computer" });
  assert.equal(hand.at(-1).value, "E:\\cards\\mine");
  assert.equal(opts(["G:\\"], true, { root: "g:", kind: "pacemaker" }).length, 2);
});

test("default pair", () => {
  const o = opts(["G:\\", "H:\\"]);
  assert.deepEqual(defaultPair(o, { root: COMP, kind: "computer" }), { from: COMP, to: "G:\\" });
  assert.deepEqual(defaultPair(o, { root: "H:\\", kind: "pacemaker" }), { from: "H:\\", to: COMP });
  assert.deepEqual(defaultPair(o, null), { from: COMP, to: "G:\\" });
  assert.deepEqual(defaultPair(opts([]), { root: COMP, kind: "computer" }), { from: "", to: "" });
  assert.deepEqual(defaultPair(opts(["G:\\"], false), null), { from: "", to: "" });
});

test("a deep link wins over the open library", () => {
  const o = opts(["G:\\", "H:\\"]);
  assert.deepEqual(defaultPair(o, { root: COMP, kind: "computer" }, { direction: "get", other: "H:\\" }), { from: "H:\\", to: COMP });
  assert.deepEqual(defaultPair(o, { root: COMP, kind: "computer" }, { direction: "send", other: "H:\\" }), { from: COMP, to: "H:\\" });
  assert.deepEqual(defaultPair(o, null, { direction: "send" }), { from: COMP, to: "G:\\" });
  // a link that cannot be followed falls back to the normal default
  assert.deepEqual(defaultPair(opts([]), null, { direction: "send", other: "G:\\" }), { from: "", to: "" });
});

test("quick buttons keep the Pacemaker already chosen", () => {
  const o = opts(["G:\\", "H:\\"]);
  assert.deepEqual(pairFor("send", o, { from: "H:\\", to: COMP }), { from: COMP, to: "H:\\" });
  assert.deepEqual(pairFor("get", o, { from: COMP, to: "H:\\" }), { from: "H:\\", to: COMP });
  assert.deepEqual(pairFor("send", o, { from: "", to: "" }), { from: COMP, to: "G:\\" });
  const same = { from: "x", to: "y" };
  assert.deepEqual(pairFor("send", opts([]), same), same);
});

test("choosing a side never leaves both the same", () => {
  const o = opts(["G:\\", "H:\\"]);
  const p = { from: COMP, to: "G:\\" };
  assert.deepEqual(pickSide("to", "H:\\", p, o), { from: COMP, to: "H:\\" });
  assert.deepEqual(pickSide("from", "G:\\", p, o), { from: "G:\\", to: COMP });
  assert.deepEqual(swapPair(p), { from: "G:\\", to: COMP });
  assert.ok(pairReady(p));
  assert.ok(!pairReady({ from: "g:", to: "G:\\" }));
  assert.ok(!pairReady({ from: "", to: "G:\\" }));
});

test("groups: removals apart, what happens first, already the same last", () => {
  const same = trk(3, { type: "skip", reason: "already the same" });
  const why = trk(2, { type: "skip", reason: "its audio file is missing" });
  const g = groupItems([same, why, trk(1), mix("m", []), cse("c", []), item("remove-track:/t/9", "track", { type: "remove" })]);
  assert.deepEqual(g.tracks.map((i) => i.key), ["track:/t/1", "track:/t/2", "track:/t/3"]);
  assert.equal(g.mixes.length, 1);
  assert.equal(g.cases.length, 1);
  assert.deepEqual(g.removals.map((i) => i.key), ["remove-track:/t/9"]);
});

test("action words", () => {
  assert.equal(actionText({ type: "copy" }), "Copy");
  assert.equal(actionText({ type: "update" }), "Update");
  assert.equal(actionText({ type: "update_plays" }), "Update play counts");
  assert.equal(actionText({ type: "repair" }), "Repair");
  assert.equal(actionText({ type: "replace" }), "Replace");
  assert.equal(actionText({ type: "copy_as_new", name: "Warm-up (2)" }), 'Copy as "Warm-up (2)"');
  assert.equal(actionText({ type: "merge" }), "Merge");
  assert.equal(actionText({ type: "skip", reason: "its audio file is missing" }), "Skip: its audio file is missing");
  assert.equal(actionText({ type: "remove" }), "Remove");
});

test("a track left out holds up the mix that needs it, and changes a case that needs it", () => {
  const items = [trk(1), trk(2), mix("Friday", [need(1), need(2)]), mix("Other", [need(2)]), cse("Opening", [need(1), need(2)])];
  const m = dependencyMarks(items, new Set(["track:/t/1"]));
  assert.deepEqual(m.get("mix:Friday"), { leaves: ["Track 1"], skipped: true });
  assert.equal(m.has("mix:Other"), false);
  assert.deepEqual(m.get("case:Opening"), { leaves: ["Track 1"], skipped: false });
  assert.equal(dependencyMarks(items, new Set()).size, 0);
  // a mix that is left out itself is not marked; a needed track the target already has is not a copy
  assert.equal(dependencyMarks(items, new Set(["track:/t/1", "mix:Friday"])).has("mix:Friday"), false);
  const there = [trk(1, { type: "update" }), mix("M", [need(1)])];
  assert.equal(dependencyMarks(there, new Set(["track:/t/1"])).size, 0);
  assert.match(markText("mix", false, m.get("mix:Friday")), /Will be skipped: it needs Track 1/);
  assert.match(markText("case", false, m.get("case:Opening")), /copied without Track 1/);
});

test("a track removal stays when the case that uses it is left out of the removals", () => {
  const rt = item("remove-track:/t/9", "track", { type: "remove" }, { label: "Old track", blockers: [{ key: "remove-case:old", label: "Old case" }] });
  const rc = item("remove-case:old", "case", { type: "remove" }, { label: "Old case" });
  assert.equal(dependencyMarks([rt, rc], new Set()).size, 0);
  const m = dependencyMarks([rt, rc], new Set(["remove-case:old"]));
  assert.deepEqual(m.get("remove-track:/t/9"), { leaves: ["Old case"], skipped: true });
  assert.deepEqual(removalList([rt, rc], new Set()), [{ kind: "Track", label: "Old track" }, { kind: "Case", label: "Old case" }]);
  assert.deepEqual(removalList([rt, rc], new Set(["remove-case:old"])), []);
});

test("totals follow the boxes", () => {
  const items = [trk(1, { type: "copy" }, 10 * MB), trk(2, { type: "update" }, 1000), mix("A", [need(1)]), mix("B", []), cse("C", [need(1)]), trk(3, { type: "skip", reason: "x" }, 5 * MB)];
  const all = tally(items, new Set());
  assert.deepEqual([all.copyTracks, all.updateTracks, all.mixes, all.cases, all.bytes], [1, 1, 2, 1, 10 * MB + 1000]);
  const less = tally(items, new Set(["track:/t/1"]));
  assert.deepEqual([less.copyTracks, less.mixes, less.skippedMixes, less.cases, less.bytes], [0, 1, 1, 1, 1000]);
  assert.equal(tallyText(less), "Will update 1 track, bring 1 mix, bring 1 case. 1 mix will be skipped.");
  assert.equal(tallyText(tally(items, new Set(items.map((i) => i.key)))), "Nothing will be changed.");
  // plays-only items are their own count, need nothing, and copy no bytes
  const p = [trk(1, { type: "update_plays" }, 0), mix("M", [need(9)]), item("case:C", "case", { type: "update_plays" }, { needs: [need(9)] })];
  p[1].action = { type: "update_plays" };
  const pt = tally(p, new Set(["track:/t/9"]));
  assert.deepEqual([pt.plays, pt.updateTracks, pt.mixes, pt.cases, pt.bytes], [3, 0, 0, 0, 0]);
  assert.equal(tallyText(pt), "Will update the play counts of 3 items.");
  assert.equal(isStale("stale plan: a library changed since the plan was made; plan again"), true);
  assert.deepEqual(excludedKeys(items, new Set(["track:/t/1", "track:/t/3", "gone"])), ["track:/t/1"]);
});

test("a repair is its own count, copies bytes, and is not an update", () => {
  const r = [trk(1, { type: "repair" }, 3 * MB), trk(2, { type: "copy" }, MB)];
  const t = tally(r, new Set());
  assert.deepEqual([t.repairTracks, t.updateTracks, t.copyTracks, t.bytes], [1, 0, 1, 4 * MB]);
  assert.equal(tallyText(t), "Will copy 1 track, copy the missing files of 1 track.");
  assert.equal(tallyText(tally(r, new Set(["track:/t/2"]))), "Will copy the missing files of 1 track.");
  assert.equal(tally([trk(1, { type: "repair" }, 5)], new Set()).bytes, 5);
  assert.deepEqual(resultParts("Copied 1 track. Repaired the missing files of 2 tracks. Updated 1 track").map((p) => p.heading || p.items.join()), ["Copied 1 track", "Repaired the missing files of 2 tracks", "Updated 1 track"]);
});

test("space line", () => {
  const plan = (free) => ({ free_bytes: free });
  assert.equal(spaceLine(plan(18.4 * GB), 1.2 * GB).text, "Needs 1.2 GB, 18.4 GB free.");
  const no = spaceLine(plan(100 * MB), 1.2 * GB);
  assert.equal(no.text, "Does not fit: needs 1.2 GB, 100.0 MB free.");
  assert.equal(no.fits, false);
  assert.equal(spaceLine(plan(5 * MB), 0).fits, true);
  assert.equal(spaceLine(plan(null), 5 * MB).fits, true);
  assert.equal(spaceLine(plan(6 * MB - 1), 5 * MB).fits, false);
  assert.equal(spaceLine(plan(6 * MB), 5 * MB).fits, true);
});

test("the finished line splits into parts, keeping the items of a list together", () => {
  const s =
    "Copied 4 tracks (35.5 MB), 1 mix. Updated 1 track. 2 items left out: Road trip: it needs Track 9, which is not on the target; Old. Mix: left out by you. 1 item could not be synced: Track 5: disk full. Tags and labels came with 4 tracks";
  const p = resultParts(s);
  assert.deepEqual(p.map((x) => x.heading), ["Copied 4 tracks (35.5 MB), 1 mix", "Updated 1 track", "2 items left out", "1 item could not be synced", "Tags and labels came with 4 tracks"]);
  assert.deepEqual(p[2].items, ["Road trip: it needs Track 9, which is not on the target", "Old. Mix: left out by you"]);
  assert.equal(p[3].tone, "bad");
  assert.deepEqual(p[3].items, ["Track 5: disk full"]);
  assert.deepEqual(resultParts("Nothing to sync, the two libraries already match").map((x) => x.heading), ["Nothing to sync, the two libraries already match"]);
});

test("the stale refusal is recognised", () => {
  assert.ok(isStale("The libraries changed since the preview, so nothing was synced. Look at the preview again."));
  assert.ok(!isStale("disk full"));
});
