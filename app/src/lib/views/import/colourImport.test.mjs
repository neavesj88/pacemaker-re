// Run: node --experimental-strip-types --test src/lib/views/import/colourImport.test.mjs   (also part of `npm test`)
import assert from "node:assert/strict";
import { test } from "node:test";
import { DEFAULT_TRAKTOR_MAP, describeChange, groupByColour, mapIndex, planColours, startingMap, summary, usedIndexes } from "./colourImport.ts";

const note = (track_id, index, title = `Title ${track_id}`) => ({ track_id, title, artist: "Example Artist", index });
const notes = [note(1, 2), note(2, 3), note(3, 3), note(4, 9), note(5, 1)];
const none = () => null;

test("the default table follows the palette for 1 to 7 and is a plain table", () => {
  assert.deepEqual(Object.keys(DEFAULT_TRAKTOR_MAP), ["1", "2", "3", "4", "5", "6", "7"]);
  assert.equal(mapIndex(DEFAULT_TRAKTOR_MAP, 1), "red");
  assert.equal(mapIndex(DEFAULT_TRAKTOR_MAP, 7), "pink");
  assert.equal(mapIndex(DEFAULT_TRAKTOR_MAP, 8), null, "an index with no entry has no colour");
  assert.equal(mapIndex({ 1: null }, 1), null);
  assert.equal(mapIndex({ 1: "mauve" }, 1), null, "only the eight label colours are ever produced");
  assert.throws(() => (DEFAULT_TRAKTOR_MAP[1] = "blue"), TypeError, "the defaults cannot be edited in place");
});

test("the table lists the indexes the file uses and starts from the defaults", () => {
  assert.deepEqual(usedIndexes(notes), [1, 2, 3, 9]);
  assert.deepEqual(startingMap([1, 3, 9]), { 1: "red", 3: "yellow", 9: null });
  assert.deepEqual(usedIndexes([]), []);
});

test("planColours: changes in file order, same / kept / unmapped counted, one decision per track", () => {
  const map = startingMap(usedIndexes(notes));
  const p = planColours(notes, map, none, false);
  assert.deepEqual(p.changes.map((c) => [c.track_id, c.from, c.to]), [[1, null, "orange"], [2, null, "yellow"], [3, null, "yellow"], [5, null, "red"]]);
  assert.deepEqual([p.same, p.kept, p.unmapped, p.replaced], [0, 0, 1, 0], "index 9 has no colour in the table");

  // existing colours: equal ones are "same", different ones are kept unless replacing is on
  const have = { 1: "orange", 2: "blue", 5: "red" };
  const keep = planColours(notes, map, (id) => have[id] ?? null, false);
  assert.deepEqual(keep.changes.map((c) => c.track_id), [3]);
  assert.deepEqual([keep.same, keep.kept, keep.unmapped], [2, 1, 1]);
  const replace = planColours(notes, map, (id) => have[id] ?? null, true);
  assert.deepEqual(replace.changes.map((c) => [c.track_id, c.from, c.to]), [[2, "blue", "yellow"], [3, null, "yellow"]]);
  assert.deepEqual([replace.same, replace.kept, replace.replaced], [2, 0, 1]);

  // a track listed twice is decided once (the plan already dedupes, but a file can repeat)
  assert.equal(planColours([note(1, 2), note(1, 3)], map, none, false).changes.length, 1);
  // clearing a table entry leaves those tracks alone
  const off = planColours(notes, { ...map, 3: null }, none, false);
  assert.deepEqual([off.changes.length, off.unmapped], [2, 3]);
});

test("changes are grouped by colour in palette order, one label call per colour", () => {
  const p = planColours(notes, startingMap(usedIndexes(notes)), none, false);
  assert.deepEqual(groupByColour(p.changes), [{ colour: "red", ids: [5] }, { colour: "orange", ids: [1] }, { colour: "yellow", ids: [2, 3] }]);
  assert.deepEqual(groupByColour([]), []);
});

test("the words name the track, the old and the new colour, and every count", () => {
  const [first] = planColours(notes, startingMap([2]), () => "blue", true).changes;
  assert.equal(describeChange(first), "Title 1 · Example Artist: Blue → Orange");
  assert.equal(describeChange({ track_id: 1, title: "", artist: "", from: null, to: "red", index: 1 }), "(untitled): no colour → Red");
  const text = summary({ changes: new Array(3).fill(0), same: 2, kept: 1, unmapped: 4, replaced: 1 });
  assert.equal(text, "3 tracks would get a colour label; 1 of them replaces a colour they already have; 2 tracks already have that colour; 1 track keeps the colour it has (replacing is off); 4 tracks have a colour index with no label colour in the table.");
  assert.equal(summary({ changes: [], same: 0, kept: 0, unmapped: 0, replaced: 0 }), "0 tracks would get a colour label.");
  assert.equal(summary({ changes: [0], same: 1, kept: 0, unmapped: 1, replaced: 0 }), "1 track would get a colour label; 1 track already has that colour; 1 track has a colour index with no label colour in the table.");
});
