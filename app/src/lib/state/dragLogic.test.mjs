// Run: node --experimental-strip-types --test src/lib/state/dragLogic.test.mjs   (also part of `npm test`)
// Pure arithmetic of the pointer drags: row drop position, lane offset, edge auto-scroll, scope.
import assert from "node:assert/strict";
import { test } from "node:test";
import { edgeScroll, insertIndex, laneOffset, rowDropAt, scopeMatches, scopeOf } from "./dragLogic.ts";

// three 20px rows with a 10px strip between each: tops 0, 30, 60
const rows = [{ top: 0, height: 20 }, { top: 30, height: 20 }, { top: 60, height: 20 }];

test("rowDropAt: halves of a row", () => {
  assert.deepEqual(rowDropAt(rows, 5), { over: 0, after: false });
  assert.deepEqual(rowDropAt(rows, 15), { over: 0, after: true });
  assert.deepEqual(rowDropAt(rows, 35), { over: 1, after: false });
  assert.deepEqual(rowDropAt(rows, 45), { over: 1, after: true });
});

test("rowDropAt: the strip between rows belongs to the row above, below it", () => {
  assert.deepEqual(rowDropAt(rows, 25), { over: 0, after: true });
});

test("rowDropAt: the ends are reachable", () => {
  assert.deepEqual(rowDropAt(rows, -40), { over: 0, after: false });
  assert.deepEqual(rowDropAt(rows, 500), { over: 2, after: true });
  assert.equal(rowDropAt([], 10), null);
});

test("insertIndex", () => {
  assert.equal(insertIndex({ over: 0, after: false }), 0);
  assert.equal(insertIndex({ over: 0, after: true }), 1);
  assert.equal(insertIndex({ over: 2, after: true }), 3);
});

test("laneOffset never goes negative", () => {
  assert.equal(laneOffset(300, 100), 200);
  assert.equal(laneOffset(50, 100), 0);
  assert.equal(laneOffset(100, 100), 0);
});

test("edgeScroll: still in the middle, faster towards the edge, nothing for tiny boxes", () => {
  assert.equal(edgeScroll(200, 0, 400), 0);
  assert.ok(edgeScroll(395, 0, 400) > 0);
  assert.ok(edgeScroll(5, 0, 400) < 0);
  assert.ok(Math.abs(edgeScroll(399, 0, 400)) > Math.abs(edgeScroll(370, 0, 400)));
  assert.equal(edgeScroll(10, 0, 50), 0);
});

test("scopeMatches: picker drags only reach mix targets, case drags only case targets (default cases)", () => {
  assert.equal(scopeMatches("mix", "mix"), true);
  assert.equal(scopeMatches(undefined, "cases"), true);
  assert.equal(scopeMatches(undefined, undefined), true);
  assert.equal(scopeMatches("mix", "cases"), false);
  assert.equal(scopeMatches(undefined, "mix"), false);
});

test("library pane drags have their own scope and reach nothing else", () => {
  assert.equal(scopeOf("lib:left:tracks"), "lib");
  assert.equal(scopeOf("lib:right:cases"), "lib");
  assert.equal(scopeMatches("lib", "lib"), true);
  assert.equal(scopeMatches("lib", "cases"), false);
  assert.equal(scopeMatches("lib", "mix"), false);
  assert.equal(scopeMatches(undefined, "lib"), false, "case targets never take a pane drag");
  assert.equal(scopeMatches("mix", "lib"), false);
});

test("scopeOf: picker drags are mix drags, the rest reach cases", () => {
  assert.equal(scopeOf("mix:picker"), "mix");
  assert.equal(scopeOf("tracks"), "cases");
  assert.equal(scopeOf("case:7"), "cases");
});
