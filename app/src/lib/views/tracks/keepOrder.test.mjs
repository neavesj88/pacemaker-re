// Run: node --experimental-strip-types --test src/lib/views/tracks/keepOrder.test.mjs
// The rule is checked against the real filter and sort: whenever it says the order is kept, the
// full pipeline on the edited library must give exactly the order it gave before.
import assert from "node:assert/strict";
import { test } from "node:test";
import { orderSurvives, pipelineFields } from "./keepOrder.ts";
import { NO_FILTER, filterTracks, sortTracksBy } from "./trackLogic.ts";

let seed = 12345;
const rnd = () => ((seed = (seed * 1664525 + 1013904223) >>> 0) / 2 ** 32);
const pick = (a) => a[Math.floor(rnd() * a.length)];

const keys = ["1A", "5A", "8B", "Am", "Em", "", "C", "F#m"];
const mk = (i) => ({
  track_id: i + 1, title: pick(["Alpha", "beta", "Gamma", "delta", "Echo"]) + (i % 7), artist: pick(["X", "Y", "Z", ""]), album: pick(["A1", "A2", ""]), genre: pick(["House", "Techno", ""]),
  bpm: pick([0, 90, 120, 120.5, 128, 174]), key: pick(keys), play_time_secs: 100 + (i % 50), rating: i % 6, location: `/t/${i}.mp3`, file_size: 1000 + (i % 13), format: pick(["MP3", "FLAC"]),
  analysed: true, times_played: i % 4, last_played: pick([-1, 5, 9]), year: pick([0, 1999, 2010]), label: pick(["", "L1"]), remixer: pick(["", "R"]), composer: pick(["", "C"]),
  comments: pick(["", "n"]), date_added: pick([-1, 3, 4]), bit_rate: pick([0, 128, 320]),
});
const FIELDS = ["title", "artist", "album", "genre", "bpm", "key", "rating", "times_played", "play_time_secs", "year", "label", "remixer", "composer", "comments", "date_added", "last_played", "format", "bit_rate", "file_size"];
const SORTS = [...FIELDS, "fav", "colour", "energy", "tags", "case_order"];
const edit = (t, f) => ({
  ...t,
  [f]: typeof t[f] === "number" ? t[f] + 1 + Math.floor(rnd() * 3) : f === "key" ? pick(keys) : t[f] + "z",
});
const run = (list, filter, sort) => {
  const fav = (id) => id % 5 === 0;
  const lab = (id) => (id % 3 === 0 ? { colour: "red", energy: 3, tags: ["a"] } : undefined);
  return sortTracksBy(filterTracks(list, filter, new Map()), sort, fav, lab).map((t) => t.track_id);
};

test("whenever the order is kept, the full pipeline agrees", () => {
  let kept = 0;
  let notKept = 0;
  for (let round = 0; round < 400; round++) {
    const prev = Array.from({ length: 40 }, (_, i) => mk(i));
    const sort = Array.from({ length: 1 + Math.floor(rnd() * 3) }, () => ({ field: pick(SORTS), asc: rnd() < 0.5 }));
    const filter = rnd() < 0.5 ? { ...NO_FILTER } : { ...NO_FILTER, genre: pick(["House", ""]), bpmMin: rnd() < 0.5 ? 100 : null, bpmMax: rnd() < 0.5 ? 150 : null, key: pick(["", "is:8B", "compat:5A"]) };
    const fields = pipelineFields(filter, sort);
    const next = prev.map((t) => (rnd() < 0.3 ? edit(t, pick(FIELDS)) : t));
    if (orderSurvives(prev, next, fields)) {
      kept++;
      assert.deepEqual(run(next, filter, sort), run(prev, filter, sort), JSON.stringify({ sort, filter }));
    } else notKept++;
  }
  assert.ok(kept > 40 && notKept > 40, `the generator should exercise both outcomes (kept ${kept}, not ${notKept})`);
});

test("a rating edit under a title sort keeps the order; under a rating sort it does not", () => {
  const prev = Array.from({ length: 5 }, (_, i) => mk(i));
  const next = prev.map((t, i) => (i === 2 ? { ...t, rating: 5 } : t));
  assert.equal(orderSurvives(prev, next, pipelineFields(NO_FILTER, [{ field: "title", asc: true }])), true);
  assert.equal(orderSurvives(prev, next, pipelineFields(NO_FILTER, [{ field: "rating", asc: false }])), false);
  assert.equal(orderSurvives(prev, next, pipelineFields({ ...NO_FILTER, key: "is:8B" }, [{ field: "artist", asc: true }])), true);
  assert.equal(orderSurvives(prev, next, null), false, "an active search query reads any field");
});

test("a different set of tracks, or a reordered one, is never kept", () => {
  const prev = Array.from({ length: 4 }, (_, i) => mk(i));
  const f = pipelineFields(NO_FILTER, [{ field: "title", asc: true }]);
  assert.equal(orderSurvives(prev, prev.slice(0, 3), f), false);
  assert.equal(orderSurvives(prev, [prev[1], prev[0], prev[2], prev[3]], f), false);
  assert.equal(orderSurvives(prev, prev, null), true);
});
