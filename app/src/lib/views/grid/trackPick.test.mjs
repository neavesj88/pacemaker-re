// Run: node --experimental-strip-types --test src/lib/views/grid/trackPick.test.mjs
import assert from "node:assert/strict";
import { test } from "node:test";
import { buildIndex, matchTracks, moveActive, SHOWN, trackLabel } from "./trackPick.ts";

const t = (id, artist, title, location = `/pmdb_tracks/${id}.mp3`) => ({ track_id: id, artist, title, location });

test("a label is artist and title, or what there is", () => {
  assert.equal(trackLabel(t(1, "Hybrid", "Finished Symphony")), "Hybrid - Finished Symphony");
  assert.equal(trackLabel(t(2, "", "Only Title")), "Only Title");
  assert.equal(trackLabel(t(3, "Only Artist", "")), "/pmdb_tracks/3.mp3");
  assert.equal(trackLabel(t(4, "", "")), "/pmdb_tracks/4.mp3");
});

test("every word must match, in any order and case, and library order is kept", () => {
  const ix = buildIndex([t(1, "Hybrid", "Finished Symphony"), t(2, "Calibre", "Even If"), t(3, "Hybrid", "Wide Angle"), t(4, "Cal", "Hybrid Moments")]);
  assert.deepEqual(matchTracks(ix, "hybrid").rows.map((r) => r.track_id), [1, 3, 4]);
  assert.deepEqual(matchTracks(ix, "SYMPH hybrid").rows.map((r) => r.track_id), [1]);
  assert.deepEqual(matchTracks(ix, "  angle   wide ").rows.map((r) => r.track_id), [3]);
  assert.deepEqual(matchTracks(ix, "nothing like it").rows, []);
  assert.equal(matchTracks(ix, "").rows.length, 4, "no words lists the library from the top");
});

test("a big library shows a page and counts the rest", () => {
  const tracks = Array.from({ length: 20000 }, (_, i) => t(i + 1, `Artist ${i % 50}`, `Song ${i}`));
  const ix = buildIndex(tracks);
  const all = matchTracks(ix, "");
  assert.equal(all.rows.length, SHOWN);
  assert.equal(all.more, 20000 - SHOWN);
  const words = ["artist", "7", "song", "9"];
  const has = (r) => words.every((w) => `${r.artist} - ${r.title}`.toLowerCase().includes(w));
  const some = matchTracks(ix, words.join(" "));
  assert.ok(some.rows.length > 0 && some.rows.every(has));
  assert.equal(some.rows.length + some.more, tracks.filter(has).length);
});

test("the arrow keys stop at the ends", () => {
  assert.equal(moveActive(-1, 1, 5), 0);
  assert.equal(moveActive(0, -1, 5), 0);
  assert.equal(moveActive(4, 1, 5), 4);
  assert.equal(moveActive(2, 1, 5), 3);
  assert.equal(moveActive(0, 1, 0), -1);
});
