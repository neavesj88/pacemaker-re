// Run: node --experimental-strip-types --test src/lib/views/mixes/tempoPrefsLogic.test.mjs
import assert from "node:assert/strict";
import { test } from "node:test";
import { DEFAULT_TEMPO_PREFS, mergeTempoPrefs } from "./tempoPrefsLogic.ts";

test("the player's memory follows the computer unless the user chose a number", () => {
  assert.equal(DEFAULT_TEMPO_PREFS.decodeAuto, true);
  assert.equal(mergeTempoPrefs(null).decodeAuto, true);
  assert.equal(mergeTempoPrefs({}).decodeAuto, true);
  assert.deepEqual([mergeTempoPrefs({ decodeAuto: false, decodeMb: 2048 }).decodeAuto, mergeTempoPrefs({ decodeAuto: false, decodeMb: 2048 }).decodeMb], [false, 2048]);
  assert.equal(mergeTempoPrefs({ decodeAuto: true, decodeMb: 2048 }).decodeAuto, true, "an explicit switch wins over the number");
});

test("settings saved before the choice existed keep a number the user typed, and read the old default as not chosen", () => {
  assert.equal(mergeTempoPrefs({ decodeMb: 1024 }).decodeAuto, true);
  const typed = mergeTempoPrefs({ decodeMb: 2048 });
  assert.equal(typed.decodeAuto, false);
  assert.equal(typed.decodeMb, 2048);
});

test("the number stays within the limits", () => {
  assert.equal(mergeTempoPrefs({ decodeAuto: false, decodeMb: 10 }).decodeMb, 256);
  assert.equal(mergeTempoPrefs({ decodeAuto: false, decodeMb: 1e9 }).decodeMb, 65536);
  assert.equal(mergeTempoPrefs({ decodeMb: Number.NaN }).decodeMb, 1024);
});
