// Run: node --experimental-strip-types src/lib/state/leaveGuard.test.mjs
import assert from "node:assert/strict";
import { allSaveable, gridLine, mixKeptNote, mixLine, registerLeaveGuard, unsavedWork } from "./leaveGuard.ts";

assert.deepEqual(unsavedWork(), [], "no guard, nothing to ask");

let mix = null;
let grid = null;
registerLeaveGuard(() => mix);
registerLeaveGuard(() => grid);
assert.deepEqual(unsavedWork(), [], "guards that return null ask nothing");

mix = { lines: [mixLine("Friday", 3)], save: async () => true };
assert.equal(unsavedWork().length, 1);
assert.deepEqual(unsavedWork()[0].lines, ["The mix “Friday”: 3 unsaved edits"], "the mix is named, with its edit count");
assert.equal(mixLine("A", 1), "The mix “A”: 1 unsaved edit");
assert.equal(mixLine("A", 0), "The mix “A”: unsaved changes", "dirty without a count still says so");
assert.ok(allSaveable(unsavedWork()));

grid = { lines: [gridLine("Song", true, false)] };
assert.equal(gridLine("Song", true, true), "The beat grid and cue point of “Song”: unsaved changes");
assert.equal(gridLine("Song", false, true), "The cue point of “Song”: unsaved changes");
const both = unsavedWork();
assert.equal(both.length, 2, "every unsaved thing is listed");
assert.deepEqual(
  both.flatMap((w) => w.lines),
  ["The mix “Friday”: 3 unsaved edits", "The beat grid of “Song”: unsaved changes"],
);
assert.equal(allSaveable(both), false, "the grid editor confirms its own write, so Save is not offered for it");
assert.equal(allSaveable([]), false);

mix = { lines: [] };
assert.equal(unsavedWork().length, 1, "a guard with no lines counts as nothing");

// a question about a case says the mix draft is not part of it (switching tabs keeps the draft)
assert.equal(mixKeptNote(null, false, 0), "");
assert.equal(mixKeptNote("Walk mix", false, 3), "", "a saved mix needs no note");
assert.match(mixKeptNote("Walk mix", true, 7), /“Walk mix” \(7 unsaved edits\) is not affected: it stays open in Mixes/);
assert.match(mixKeptNote("Walk mix", true, 0), /“Walk mix” is not affected/);

console.log("leaveGuard.test.mjs ok");
