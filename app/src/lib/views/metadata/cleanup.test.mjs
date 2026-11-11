// Run: node src/lib/views/metadata/cleanup.test.mjs   (Node >= 22.6 strips the types itself)
// Holds the TypeScript port of the clean-up recipes (devmock/cleanupEngine.ts) to the same table
// the Rust engine passes (cleanupGolden.json, `golden_cases` in cleanup.rs), and tests the pure
// helpers of the clean-up dialog.
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { TEXT_FIELDS, cleanFields } from "../../devmock/cleanupEngine.ts";
import {
  NUMERIC_TAG_FIELDS, acceptedOf, diffParts, fieldsByTrack, formatList, formatMap, idsOf, itemsFromDetails, itemsFromPlans, itemsOf, parseList, parseMap, tally, tickKey,
} from "./cleanupLogic.ts";

const here = dirname(fileURLToPath(import.meta.url));
const golden = JSON.parse(readFileSync(join(here, "../../devmock/cleanupGolden.json"), "utf8"));

test("every golden case gives the same answer as the Rust engine", () => {
  assert.ok(golden.cases.length >= 30);
  const none = { ...golden.defaults, trim: false, strip_number: false, strip_promo: false, feat_normalise: false };
  for (const c of golden.cases) {
    const cur = Object.fromEntries(TEXT_FIELDS.map((f) => [f, ""]));
    Object.assign(cur, c.input);
    const now = cleanFields(cur, { ...none, ...c.settings });
    const changed = Object.fromEntries(TEXT_FIELDS.filter((f) => cur[f] !== now[f]).map((f) => [f, now[f]]));
    assert.deepEqual(changed, c.expect, `case: ${c.name}`);
  }
});

test("lists parse one entry per line without repeats", () => {
  assert.deepEqual(parseList(" DJ \n\nUK\r\ndj\nVIP "), ["DJ", "UK", "VIP"]);
  assert.equal(formatList(["a", "b"]), "a\nb");
  assert.deepEqual(parseList(""), []);
});

test("maps parse arrows, ignore broken lines and count them", () => {
  const { entries, ignored } = parseMap("Tech-House -> Tech House\nDnB => Drum & Bass\nHip Hop \u2192 Hip-Hop\nno arrow\n -> x\ny ->\n\nTech-house -> Other");
  assert.deepEqual(entries, [
    { from: "Tech-House", to: "Tech House" },
    { from: "DnB", to: "Drum & Bass" },
    { from: "Hip Hop", to: "Hip-Hop" },
  ]);
  assert.equal(ignored, 3, "no arrow, empty name, empty target");
  assert.equal(formatMap(entries), "Tech-House -> Tech House\nDnB -> Drum & Bass\nHip Hop -> Hip-Hop");
  assert.deepEqual(parseMap(formatMap(entries)).entries, entries, "format then parse is stable");
  assert.deepEqual(parseMap("a -> b -> c").entries, [{ from: "a", to: "b -> c" }], "the first arrow splits");
});

const preview = {
  scanned: 3,
  rows: [
    { track_id: 1, artist: "A", title: "t", changes: [
      { field: "title", old: " t ", new: "t", problem: null },
      { field: "genre", old: "x", new: "X", problem: null },
    ] },
    { track_id: 2, artist: "B", title: "u", changes: [{ field: "artist", old: "b", new: "B", problem: "artist would be longer than 255 characters" }] },
    { track_id: 3, artist: "C", title: "v", changes: [{ field: "title", old: "v ", new: "v", problem: null }] },
  ],
};

test("ticks: unticked and blocked changes are not accepted", () => {
  const off = { [tickKey(1, "genre")]: true };
  assert.deepEqual(acceptedOf(preview, off), [
    { track_id: 1, field: "title", old: " t ", new: "t" },
    { track_id: 3, field: "title", old: "v ", new: "v" },
  ]);
  assert.deepEqual(tally(preview, off), { tracks: 2, fields: 2, blocked: 1 });
  assert.deepEqual(tally(preview, {}), { tracks: 2, fields: 3, blocked: 1 });
  assert.deepEqual(acceptedOf(null, {}), []);
});

test("accepted changes become one item per track", () => {
  const items = itemsOf(acceptedOf(preview, {}));
  assert.deepEqual(items, [{ track_id: 1, title: "t", genre: "X" }, { track_id: 3, title: "t".replace("t", "v") }]);
});

test("tag plans become items: only write plans, only ticked fields, numbers as numbers", () => {
  const plans = [
    { track_id: 1, status: "write", changes: [{ field: "title", old: "", new: "T" }, { field: "year", old: "", new: "2001" }, { field: "genre", old: "", new: "G" }] },
    { track_id: 2, status: "same", changes: [] },
    { track_id: 3, status: "unsupported", changes: [] },
    { track_id: 4, status: "write", changes: [{ field: "title", old: "a", new: "b" }] },
  ];
  const items = itemsFromPlans(plans, { [tickKey(1, "genre")]: true, [tickKey(4, "title")]: true }, NUMERIC_TAG_FIELDS);
  assert.deepEqual(items, [{ track_id: 1, title: "T", year: 2001 }]);
});

test("diff finds the middle that changed, also for white space and surrogate pairs", () => {
  assert.deepEqual(diffParts("abc", "abc"), { pre: "abc", del: "", add: "", post: "" });
  assert.deepEqual(diffParts("one  two", "one two"), { pre: "one ", del: " ", add: "", post: "two" });
  assert.deepEqual(diffParts("x (Free Download)", "x"), { pre: "x", del: " (Free Download)", add: "", post: "" });
  assert.deepEqual(diffParts("", "A"), { pre: "", del: "", add: "A", post: "" });
  assert.deepEqual(diffParts("a\u{1F3B5}b", "a\u{1F3B6}b"), { pre: "a", del: "\u{1F3B5}", add: "\u{1F3B6}", post: "b" });
  for (const [a, b] of [["  x ", "x"], ["Tech-House", "Tech House"], ["a", ""]]) {
    const p = diffParts(a, b);
    assert.equal(p.pre + p.del + p.post, a);
    assert.equal(p.pre + p.add + p.post, b);
  }
});

test("library rows become tag items: chosen fields only, blanks and zeros left out", () => {
  const rows = [
    { track_id: 1, title: "T", artist: "", genre: "G", year: 2001, track_number: 0 },
    { track_id: 2, title: "", artist: "", genre: "", year: 0 },
  ];
  assert.deepEqual(itemsFromDetails(rows, ["title", "artist", "year", "track_number"]), [{ track_id: 1, title: "T", year: 2001 }]);
  assert.deepEqual(itemsFromDetails(rows, []), []);
});

test("accepted changes: ids once each, fields per track", () => {
  const acc = [{ track_id: 5, field: "title" }, { track_id: 5, field: "genre" }, { track_id: 7, field: "title" }, { track_id: 5, field: "title" }];
  assert.deepEqual(idsOf(acc), [5, 7]);
  assert.deepEqual([...fieldsByTrack(acc)], [[5, ["title", "genre"]], [7, ["title"]]]);
});
