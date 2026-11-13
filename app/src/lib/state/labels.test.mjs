// Run: node --experimental-strip-types --test src/lib/state/labels.test.mjs   (also part of `npm test`)
// Tests of the pure label logic (labels.ts). The parity vectors are the same file the Rust tests
// of commands/tags/store.rs run, so the preview mirror and the real store cannot drift apart.
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import * as L from "./labels.ts";

const parity = JSON.parse(readFileSync(fileURLToPath(new URL("./labels.parity.json", import.meta.url)), "utf8"));
const none = { tags: [], colour: null, energy: null };
const lab = (tags = [], colour = null, energy = null) => ({ tags, colour, energy });

test("applyOps gives the same answer as the Rust store on every shared vector", () => {
  assert.ok(parity.length >= 10);
  for (const c of parity) {
    const r = L.applyOps(c.before, c.ops);
    if (c.after === null) assert.equal(r.ok, false, c.name);
    else {
      assert.equal(r.ok, true, `${c.name}: ${r.error}`);
      assert.deepEqual(r.labels, c.after, c.name);
    }
  }
});

test("applyOps never changes its input", () => {
  const before = lab(["b", "a"], "red", 3);
  const copy = structuredClone(before);
  L.applyOps(before, [{ op: "add_tags", tags: ["z"] }, { op: "set_colour", colour: null }]);
  assert.deepEqual(before, copy);
});

test("normaliseTag and parseTagInput", () => {
  assert.deepEqual(L.normaliseTag("  deep   house "), { ok: true, tag: "deep house" });
  assert.equal(L.normaliseTag("a,b").ok, false);
  assert.equal(L.normaliseTag(" ").ok, false);
  assert.equal(L.normaliseTag("x".repeat(41)).ok, false);
  assert.equal(L.normaliseTag("x".repeat(40)).ok, true);
  const r = L.parseTagInput("warm up, Vocal,\nvocal ; ,closer,a\u0001b");
  assert.deepEqual(r.tags, ["warm up", "Vocal", "closer"], "repeats ignoring case are kept once");
  assert.equal(r.errors.length, 1);
});

test("the palette is the eight named colours", () => {
  assert.deepEqual([...L.LABEL_COLOURS], ["red", "orange", "yellow", "green", "teal", "blue", "purple", "pink"]);
  assert.equal(L.colourName("teal"), "Teal");
  assert.equal(L.colourVar("pink"), "var(--label-pink)");
  assert.ok(L.isColour("red") && !L.isColour("mauve") && !L.isColour(3));
  assert.equal(L.colourRank("red"), 0);
  assert.equal(L.colourRank(null), 999);
});

test("vocabulary counts tracks, uses the most common spelling and sorts by name", () => {
  const v = L.tagVocabulary([lab(["house", "Vocal"]), lab(["House"]), lab(["House", "closer"]), none]);
  assert.deepEqual(v, [
    { tag: "closer", count: 1 },
    { tag: "House", count: 3 },
    { tag: "Vocal", count: 1 },
  ]);
  assert.equal(L.canonicalTag(v, "vocal"), "Vocal");
  assert.equal(L.canonicalTag(v, "brand new"), "brand new");
});

test("suggestions: prefix first, then contains; excluded and empty input", () => {
  const v = [{ tag: "Deep", count: 2 }, { tag: "Deep house", count: 1 }, { tag: "Undeep", count: 1 }, { tag: "peak", count: 5 }];
  assert.deepEqual(L.suggestTags(v, "dee").map((s) => s.tag), ["Deep", "Deep house", "Undeep"]);
  assert.deepEqual(L.suggestTags(v, "dee", new Set(["deep"])).map((s) => s.tag), ["Deep house", "Undeep"]);
  assert.equal(L.suggestTags(v, "").length, 4);
  assert.deepEqual(L.suggestTags(v, "zzz"), []);
  assert.equal(L.suggestTags(v, "", new Set(), 2).length, 2);
});

test("tracksWithTag finds a tag ignoring case", () => {
  const entries = [[1, lab(["Peak"])], [2, lab(["peak", "x"])], [3, lab(["other"])]];
  assert.deepEqual(L.tracksWithTag(entries, "PEAK"), [1, 2]);
  assert.deepEqual(L.tracksWithTag(entries, "none"), []);
});

test("label filter: tags any / all", () => {
  const f = { ...L.freshLabelFilter(), tags: ["Peak", "vocal"] };
  const a = lab(["peak"]);
  const b = lab(["Peak", "Vocal"]);
  assert.equal(L.matchesLabels(a, f), true, "any");
  assert.equal(L.matchesLabels(none, f), false);
  assert.equal(L.matchesLabels(undefined, f), false);
  f.tagMode = "all";
  assert.equal(L.matchesLabels(a, f), false);
  assert.equal(L.matchesLabels(b, f), true);
});

test("label filter: colours, energy range, and combinations", () => {
  const red = lab([], "red", 7);
  assert.equal(L.matchesLabels(red, { ...L.freshLabelFilter(), colours: ["red", "blue"] }), true);
  assert.equal(L.matchesLabels(red, { ...L.freshLabelFilter(), colours: ["blue"] }), false);
  assert.equal(L.matchesLabels(lab(), { ...L.freshLabelFilter(), colours: ["blue"] }), false, "no colour never matches a colour");
  const range = (min, max) => ({ ...L.freshLabelFilter(), energyMin: min, energyMax: max });
  assert.equal(L.matchesLabels(red, range(7, 7)), true);
  assert.equal(L.matchesLabels(red, range(8, null)), false);
  assert.equal(L.matchesLabels(red, range(null, 6)), false);
  assert.equal(L.matchesLabels(red, range(null, 7)), true);
  assert.equal(L.matchesLabels(lab(["x"]), range(1, 10)), false, "an unrated track never matches an energy range");
  const both = { ...L.freshLabelFilter(), colours: ["red"], energyMin: 5, tags: ["x"] };
  assert.equal(L.matchesLabels(lab(["x"], "red", 6), both), true);
  assert.equal(L.matchesLabels(lab(["x"], "red", 4), both), false);
  assert.equal(L.matchesLabels(lab([], "red", 6), both), false);
});

test("filterByLabels is a no-op without a filter and keeps order", () => {
  const rows = [{ track_id: 3 }, { track_id: 1 }, { track_id: 2 }];
  const get = (id) => (id === 1 ? lab([], "red") : id === 3 ? lab([], "red") : undefined);
  assert.equal(L.filterByLabels(rows, L.freshLabelFilter(), get), rows);
  assert.deepEqual(L.filterByLabels(rows, { ...L.freshLabelFilter(), colours: ["red"] }, get).map((r) => r.track_id), [3, 1]);
  assert.equal(L.labelFilterActive(L.freshLabelFilter()), false);
  assert.equal(L.labelFilterActive({ ...L.freshLabelFilter(), energyMax: 3 }), true);
  assert.equal(L.labelFilterActive(L.NO_LABEL_FILTER), false);
});

test("describeLabelFilter words the conditions", () => {
  assert.equal(L.describeLabelFilter(L.freshLabelFilter()), "");
  const s = L.describeLabelFilter({ tags: ["a"], tagMode: "all", colours: ["red", "pink"], energyMin: 4, energyMax: null });
  assert.ok(s.includes("all of tags “a”") && s.includes("Red or Pink") && s.includes("energy 4 to 10"), s);
});

test("tagPresence counts how many targets carry each tag", () => {
  const p = L.tagPresence([lab(["a", "B"]), lab(["b"]), none]);
  assert.deepEqual(p, [{ tag: "a", have: 1 }, { tag: "B", have: 2 }]);
});

test("commonValue", () => {
  assert.deepEqual(L.commonValue(["red", "red"]), { kind: "same", value: "red" });
  assert.deepEqual(L.commonValue([null, null]), { kind: "same", value: null });
  assert.deepEqual(L.commonValue(["red", null]), { kind: "mixed" });
  assert.deepEqual(L.commonValue([]), { kind: "mixed" });
});

test("drafts: queued tags cancel each other and become ops in a fixed order", () => {
  const d = L.emptyDraft();
  assert.equal(L.draftIsEmpty(d), true);
  L.draftAddTag(d, "new");
  L.draftRemoveTag(d, "old");
  d.colour = null;
  d.energy = 5;
  assert.deepEqual(L.draftOps(d), [
    { op: "remove_tags", tags: ["old"] },
    { op: "add_tags", tags: ["new"] },
    { op: "set_colour", colour: null },
    { op: "set_energy", energy: 5 },
  ]);
  // removing a tag that is only queued for adding turns the queue entry into a removal
  L.draftRemoveTag(d, "NEW");
  assert.deepEqual(d.add, []);
  assert.deepEqual(d.remove.map((t) => t.toLowerCase()), ["old", "new"]);
  L.draftForgetTag(d, "new");
  assert.deepEqual(d.remove, ["old"], "forgetting takes a queued tag back whichever list it is in");
  L.draftAddTag(d, "OLD");
  assert.deepEqual(d.remove, [], "adding a tag queued for removal cancels the removal");
  assert.deepEqual(d.add, ["OLD"]);
});

test("describeChanges uses exact counts from the same rules as the action", () => {
  const targets = [lab(["a"], "red", 3), lab(["a", "b"], null, 3), lab([], "red", null)];
  const d = L.emptyDraft();
  d.remove = ["a", "zzz"];
  d.add = ["b", "c"];
  d.colour = "red";
  d.energy = 3;
  assert.deepEqual(L.describeChanges(d, targets), [
    "Remove tag “a” from 2 of 3 tracks",
    "Remove tag “zzz”: none of the 3 tracks has it",
    "Add tag “b” to 2 of 3 tracks",
    "Add tag “c” to 3 of 3 tracks",
    "Set colour Red on 1 of 3 tracks",
    "Set energy 3 on 1 of 3 tracks",
  ]);
  const clear = { ...L.emptyDraft(), colour: null, energy: null };
  assert.deepEqual(L.describeChanges(clear, targets), ["Clear the colour on 2 of 3 tracks", "Clear the energy on 2 of 3 tracks"]);
  assert.deepEqual(L.describeChanges(L.emptyDraft(), targets), []);
  assert.deepEqual(L.describeChanges({ ...L.emptyDraft(), add: ["a"] }, [lab(["a"])]), ["Add tag “a”: the track has it already"]);
});

test("draftChangesAnything ignores a draft that repeats what the tracks have", () => {
  const t = [lab(["a"], "red", 4)];
  assert.equal(L.draftChangesAnything(L.emptyDraft(), t), false);
  assert.equal(L.draftChangesAnything({ ...L.emptyDraft(), colour: "red", energy: 4, add: ["A"] }, t), false);
  assert.equal(L.draftChangesAnything({ ...L.emptyDraft(), energy: 5 }, t), true);
  assert.equal(L.draftChangesAnything({ ...L.emptyDraft(), remove: ["a"] }, t), true);
  assert.equal(L.draftChangesAnything({ ...L.emptyDraft(), add: ["bad,tag"] }, t), false, "an invalid draft is not applied");
});

test("labelsEmpty", () => {
  assert.ok(L.labelsEmpty(undefined) && L.labelsEmpty(none) && !L.labelsEmpty(lab(["x"])) && !L.labelsEmpty(lab([], null, 1)));
});

test("orphansWithTag picks the removed tracks' labels carrying a tag, ignoring case", () => {
  const gone = [lab(["Opener", "x"]), lab(["peak"]), lab([], "red", 3)];
  assert.deepEqual(L.orphansWithTag(gone, "OPENER"), [gone[0]]);
  assert.deepEqual(L.orphansWithTag(gone, "none"), []);
  assert.deepEqual(L.orphansWithTag([], "x"), []);
});

test("describeOrphan names a removed track by its labels", () => {
  assert.equal(L.describeOrphan(lab(["opener", "old crate"], "teal", 4)), "opener, old crate · Teal · energy 4");
  assert.equal(L.describeOrphan(lab([], null, 7)), "energy 7");
  assert.equal(L.describeOrphan(lab([], "pink", null)), "Pink");
  assert.equal(L.describeOrphan(none), "no labels");
});

test("describeChanges names the kind of thing: tracks by default, cases and mixes when asked", () => {
  const targets = [lab(["peak"], "red"), lab([], null), lab(["peak"], null)];
  const d = { add: ["closer"], remove: ["peak"], colour: null, energy: undefined };
  assert.deepEqual(L.describeChanges(d, targets), [
    "Remove tag “peak” from 2 of 3 tracks",
    "Add tag “closer” to 3 of 3 tracks",
    "Clear the colour on 1 of 3 tracks",
  ]);
  assert.deepEqual(L.describeChanges({ add: ["peak"], remove: [], colour: "teal", energy: undefined }, [lab(["peak"], "teal")], "case"), [
    "Add tag “peak”: the case has it already",
    "Set colour Teal on 0 of 1 case",
  ]);
  assert.deepEqual(L.describeChanges({ add: ["x"], remove: [], colour: undefined, energy: undefined }, [lab(["x"]), lab(["X"])], "mix"), ["Add tag “x”: all 2 mixes have it already"]);
  assert.deepEqual(L.describeChanges({ add: [], remove: ["x"], colour: undefined, energy: undefined }, [lab([]), lab([])], "mix"), ["Remove tag “x”: none of the 2 mixes has it"]);
  assert.equal(L.thingsWord("track"), "tracks");
  assert.equal(L.thingsWord("case"), "cases");
  assert.equal(L.thingsWord("mix"), "mixes");
});
