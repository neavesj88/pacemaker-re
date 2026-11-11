// Run: node src/lib/views/tracks/viewState.test.mjs
import assert from "node:assert/strict";
import { test } from "node:test";
import { describeViewState, emptyViewState, sameViewState, sanitiseViewState, viewStateIsClear } from "./viewState.ts";

test("an empty state is clear and survives sanitising unchanged", () => {
  const e = emptyViewState();
  assert.ok(viewStateIsClear(e));
  assert.deepEqual(sanitiseViewState(JSON.parse(JSON.stringify(e))), e);
  assert.equal(describeViewState(e), "no filters");
});

test("a full state round trips through JSON", () => {
  const s = {
    query: "bpm:120-128 key:~8A",
    onlyNeeds: false,
    onlyAnalysed: true,
    genre: "House",
    bpmMin: "120",
    bpmMax: "128.5",
    keyFilter: "compat:8A",
    labelFilter: { tags: ["peak", "warm up"], tagMode: "all", colours: ["red", "blue"], energyMin: 3, energyMax: 9 },
    sort: [{ field: "bpm", asc: false }, { field: "title", asc: true }],
  };
  assert.deepEqual(sanitiseViewState(JSON.parse(JSON.stringify(s))), s);
  assert.ok(!viewStateIsClear(s));
});

test("anything that is not an object is rejected; wrong types become empty values", () => {
  for (const bad of [null, undefined, 3, "x", [], [1]]) assert.equal(sanitiseViewState(bad), null);
  const s = sanitiseViewState({ query: 5, genre: [], onlyNeeds: "yes", bpmMin: {}, keyFilter: 3, labelFilter: "x", sort: "x" });
  assert.deepEqual(s, emptyViewState());
});

test("limits and vocabularies are enforced on read", () => {
  const s = sanitiseViewState({
    query: "q".repeat(5000),
    genre: "g".repeat(5000),
    bpmMin: "1".repeat(50),
    keyFilter: "compat:13A",
    labelFilter: { tags: ["ok", "", "  ", "x".repeat(41), 7, ...Array.from({ length: 60 }, (_, i) => `t${i}`)], tagMode: "weird", colours: ["red", "mauve", "red", 4], energyMin: 0, energyMax: 11 },
  });
  assert.equal(s.query.length, 500);
  assert.equal(s.genre.length, 200);
  assert.equal(s.bpmMin.length, 12);
  assert.equal(s.keyFilter, "", "13A is not a Camelot key");
  assert.equal(s.labelFilter.tags.length, 32);
  assert.equal(s.labelFilter.tags[0], "ok");
  assert.ok(!s.labelFilter.tags.includes(""));
  assert.equal(s.labelFilter.tagMode, "any");
  assert.deepEqual(s.labelFilter.colours, ["red"]);
  assert.equal(s.labelFilter.energyMin, null);
  assert.equal(s.labelFilter.energyMax, null);
  for (const k of ["compat:8A", "is:12B", "compat:1A", ""]) assert.equal(sanitiseViewState({ keyFilter: k }).keyFilter, k);
  for (const k of ["compat:8", "is:8C", "other:8A", "compat:08A", "compat:8A "]) assert.equal(sanitiseViewState({ keyFilter: k }).keyFilter, "", k);
});

test("analysed and missing are opposites: a file with both on keeps neither", () => {
  const s = sanitiseViewState({ onlyNeeds: true, onlyAnalysed: true });
  assert.equal(s.onlyNeeds, false);
  assert.equal(s.onlyAnalysed, false);
});

test("the sort is cleaned and only present when something usable is left", () => {
  assert.equal("sort" in sanitiseViewState({ sort: [{ field: "nope", asc: true }] }), false);
  assert.deepEqual(sanitiseViewState({ sort: [{ field: "year", asc: true }, { field: "year", asc: false }] }).sort, [{ field: "year", asc: true }]);
});

test("same state ignores key order; the sort counts", () => {
  const a = sanitiseViewState({ query: "x", genre: "House" });
  const b = sanitiseViewState({ genre: "House", query: "x" });
  assert.ok(sameViewState(a, b));
  assert.ok(!sameViewState(a, { ...b, query: "y" }));
  assert.ok(!sameViewState(a, { ...b, sort: [{ field: "bpm", asc: true }] }));
  assert.ok(viewStateIsClear({ ...emptyViewState(), sort: [{ field: "bpm", asc: true }] }), "a sort alone filters nothing");
});

test("the description names every condition", () => {
  const s = sanitiseViewState({
    query: "peak", genre: "House", bpmMin: "120", bpmMax: "", keyFilter: "is:8A", onlyNeeds: true,
    labelFilter: { tags: ["a"], tagMode: "any", colours: ["red"], energyMin: 7, energyMax: null },
    sort: [{ field: "bpm", asc: true }, { field: "title", asc: false }],
  });
  const text = describeViewState(s, (k) => k.field.toUpperCase());
  for (const part of ["search “peak”", "genre House", "BPM 120 to any", "key 8A", "tags “a”", "colour Red", "energy 7 to 10", "only missing a beat grid", "sorted by BPM ↑, TITLE ↓"]) {
    assert.ok(text.includes(part), `${part} in ${text}`);
  }
  assert.ok(describeViewState({ ...s, keyFilter: "compat:8A" }).includes("keys compatible with 8A"));
  assert.ok(describeViewState({ ...s, onlyNeeds: false, onlyAnalysed: true }).includes("only analysed"));
});
