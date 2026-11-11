// Run: node --experimental-strip-types --test src/lib/state/tempoLogic.test.mjs
import assert from "node:assert/strict";
import { test } from "node:test";
import { changing, fmtBpm, MAX_BPM, MIN_BPM, nothingToDo, opDescription, opTitle, parseTempo, rowLine } from "./tempoLogic.ts";

test("a typed tempo is read with a comma or a point and kept inside the allowed range", () => {
  assert.deepEqual(parseTempo("128"), { bpm: 128 });
  assert.deepEqual(parseTempo(" 127,5 "), { bpm: 127.5 });
  assert.deepEqual(parseTempo("127.456"), { bpm: 127.46 });
  assert.deepEqual(parseTempo(String(MIN_BPM)), { bpm: MIN_BPM });
  assert.deepEqual(parseTempo(String(MAX_BPM)), { bpm: MAX_BPM });
  for (const bad of ["", "  ", "abc", "19.99", "400.01", "-120", "0", "NaN", "Infinity", "12 8"]) {
    assert.ok("error" in parseTempo(bad), `${JSON.stringify(bad)} must be refused`);
  }
  assert.match(parseTempo("500").error, /between 20 and 400/);
});

const change = { track_id: 1, label: "A - One", before: 172, after: 86, skip: null };
const left = { track_id: 2, label: "A - Two", before: 30, after: null, skip: "20.00 BPM is below the minimum of 20" };

test("each row says in words what happens to the track", () => {
  assert.equal(rowLine(change), "A - One: 172.00 → 86.00 BPM");
  assert.equal(rowLine(left), "A - Two: left as it is (20.00 BPM is below the minimum of 20)");
  assert.equal(fmtBpm(127.5), "127.50");
});

test("only rows without a reason are counted as changes", () => {
  assert.deepEqual(changing([change, left]).map((r) => r.track_id), [1]);
  assert.deepEqual(changing([left]), []);
});

test("the wording names the edit, and the Set wording warns that the grid is replaced", () => {
  assert.equal(opTitle({ kind: "halve" }, 3, "P:\\"), "Halve the BPM of 3 tracks on P:\\ ?");
  assert.equal(opTitle({ kind: "double" }, 1, "/mnt/pm"), "Double the BPM of 1 track on /mnt/pm ?");
  assert.equal(opTitle({ kind: "set", bpm: 127.5 }, 1, "P:\\"), "Set the BPM of 1 track to 127.50 on P:\\ ?");
  assert.match(opDescription({ kind: "set", bpm: 128 }), /Tempo changes inside the track are lost/);
  assert.match(opDescription({ kind: "halve" }), /every second beat line is kept/);
  for (const op of [{ kind: "halve" }, { kind: "double" }, { kind: "set", bpm: 100 }]) {
    assert.match(opDescription(op), /rewritten together/, "every edit says BPM and grid change together");
  }
});

test("when nothing would change the message gives the reasons once each", () => {
  assert.equal(nothingToDo([]), "No tracks to change.");
  const same = { ...left, skip: "the grid is already like that" };
  assert.equal(nothingToDo([same, { ...same, track_id: 3 }]), "Nothing would change: the grid is already like that.");
  const four = [1, 2, 3, 4].map((i) => ({ ...left, track_id: i, skip: `reason ${i}` }));
  assert.equal(nothingToDo(four), "Nothing would change: reason 1; reason 2; reason 3; ….");
});
