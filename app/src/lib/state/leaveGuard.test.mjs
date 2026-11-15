// Run: node --experimental-strip-types src/lib/state/leaveGuard.test.mjs
import assert from "node:assert/strict";
import { allSaveable, gridLine, mixKeptNote, mixLine, registerLeaveGuard, unsavedWork } from "./leaveGuard.ts";

assert.deepEqual(unsavedWork(), [], "no guard, nothing to ask");

let mix = null;
let grid = null;
registerLeaveGuard(() => mix);
registerLeaveGuard(() => grid);
assert.deepEqual(unsavedWork(), [], "guards that return null ask nothing");

mix = { lines: [mixLine("Friday")], save: async () => true };
assert.equal(unsavedWork().length, 1);
assert.deepEqual(unsavedWork()[0].lines, ["The mix “Friday”: unsaved changes"], "the mix is named");
assert.equal(mixLine("A"), "The mix “A”: unsaved changes", "no count: undo depth is not edits since the last save");
assert.ok(allSaveable(unsavedWork()));

grid = { lines: [gridLine("Song", true, false)] };
assert.equal(gridLine("Song", true, true), "The beat grid and cue point of “Song”: unsaved changes");
assert.equal(gridLine("Song", false, true), "The cue point of “Song”: unsaved changes");
const both = unsavedWork();
assert.equal(both.length, 2, "every unsaved thing is listed");
assert.deepEqual(
  both.flatMap((w) => w.lines),
  ["The mix “Friday”: unsaved changes", "The beat grid of “Song”: unsaved changes"],
);
assert.equal(allSaveable(both), false, "the grid editor confirms its own write, so Save is not offered for it");
assert.equal(allSaveable([]), false);

mix = { lines: [] };
assert.equal(unsavedWork().length, 1, "a guard with no lines counts as nothing");

// a question about a case says the mix draft is not part of it (switching tabs keeps the draft)
assert.equal(mixKeptNote(null, false), "");
assert.equal(mixKeptNote("Walk mix", false), "", "a saved mix needs no note");
assert.match(mixKeptNote("Walk mix", true), /“Walk mix” has unsaved changes, but it is not affected: it stays open in Mixes/);

console.log("leaveGuard.test.mjs ok");
