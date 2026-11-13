// Run: node --experimental-strip-types --test src/lib/state/keyLogic.test.mjs
import assert from "node:assert/strict";
import { test } from "node:test";
import { parseKey } from "../views/tracks/trackLogic.ts";
import {
  canonicalKeyName,
  clampPct,
  confidenceFloor,
  DEFAULT_KEY_SETTINGS,
  detectionPlan,
  detectLine,
  normaliseKeySettings,
  PICKER_KEYS,
  pickerKeyOf,
  spellings,
} from "./keyLogic.ts";

test("the picker has all 24 keys once, in Camelot order", () => {
  assert.equal(PICKER_KEYS.length, 24);
  assert.equal(new Set(PICKER_KEYS.map((k) => k.name)).size, 24, "no key twice");
  assert.equal(new Set(PICKER_KEYS.map((k) => k.camelot)).size, 24);
  assert.deepEqual(
    PICKER_KEYS.slice(0, 4).map((k) => k.camelot),
    ["1A", "1B", "2A", "2B"],
  );
  assert.equal(PICKER_KEYS.at(-1).camelot, "12B");
});

test("the picker's Camelot codes agree with the table's own key parser", () => {
  for (const k of PICKER_KEYS) assert.equal(parseKey(k.name)?.code, k.camelot, k.name);
  for (const k of PICKER_KEYS) if (k.flat) assert.equal(parseKey(k.flat)?.code, k.camelot, k.flat);
  // spot checks against the wheel: C = 8B, Am = 8A, F#m = 11A, Ab major = 4B
  const code = (n) => PICKER_KEYS.find((k) => k.name === n).camelot;
  assert.deepEqual([code("C"), code("Am"), code("F#m"), code("G#")], ["8B", "8A", "11A", "4B"]);
});

test("every stored name is the detector's spelling and fits the 4-character column", () => {
  for (const k of PICKER_KEYS) {
    assert.match(k.name, /^[A-G]#?m?$/, k.name);
    assert.ok(k.name.length <= 4);
  }
  assert.equal(spellings(PICKER_KEYS.find((k) => k.name === "A#m")), "A#m / Bbm");
  assert.equal(spellings(PICKER_KEYS.find((k) => k.name === "Am")), "Am");
});

test("typed keys are stored the way the backend's canonical_key stores them", () => {
  const want = {
    Am: "Am", " bb ": "A#", Ebm: "D#m", "A minor": "Am", "c major": "C", "F#m": "F#m", "f♯ minor": "F#m", "B♭": "A#",
    Cb: "B", "E#": "F", Gb: "F#",
  };
  for (const [text, name] of Object.entries(want)) assert.equal(canonicalKeyName(text), name, JSON.stringify(text));
  // a bare capital M ("CM") is major in some books and minor in others: refused, not guessed
  for (const bad of ["", "  ", "8A", "12B", "H", "C#maj7", "Am7", "key", "A  m  m", "CM", "F#M", "A M"]) assert.equal(canonicalKeyName(bad), null, JSON.stringify(bad));
});

test("pickerKeyOf finds the entry for any spelling and nothing for junk", () => {
  assert.equal(pickerKeyOf("Ab Minor")?.name, "G#m");
  assert.equal(pickerKeyOf("Cm")?.camelot, "5A");
  assert.equal(pickerKeyOf(""), null);
  assert.equal(pickerKeyOf("8A"), null);
});

test("settings default to the safe choice and survive junk in storage", () => {
  assert.deepEqual(DEFAULT_KEY_SETTINGS, { overwrite: false, minConfidencePct: 0 });
  assert.deepEqual(normaliseKeySettings(null), DEFAULT_KEY_SETTINGS);
  assert.deepEqual(normaliseKeySettings("text"), DEFAULT_KEY_SETTINGS);
  assert.deepEqual(normaliseKeySettings({ overwrite: "yes", minConfidencePct: "40" }), DEFAULT_KEY_SETTINGS);
  assert.deepEqual(normaliseKeySettings({ overwrite: true, minConfidencePct: 35 }), { overwrite: true, minConfidencePct: 35 });
  assert.equal(normaliseKeySettings({ minConfidencePct: 250 }).minConfidencePct, 100);
  assert.equal(normaliseKeySettings({ minConfidencePct: -3 }).minConfidencePct, 0);
  assert.equal(normaliseKeySettings({ minConfidencePct: NaN }).minConfidencePct, 0);
  assert.equal(clampPct(33.6), 34);
});

test("the confidence floor is the per-cent setting as 0..1", () => {
  assert.equal(confidenceFloor({ overwrite: false, minConfidencePct: 0 }), 0);
  assert.equal(confidenceFloor({ overwrite: false, minConfidencePct: 40 }), 0.4);
  assert.equal(confidenceFloor({ overwrite: false, minConfidencePct: 999 }), 1);
});

const t = (id, key) => ({ track_id: id, title: `Track ${id}`, artist: "", key, location: `/pmdb_tracks/0/0/f${id}.mp3` });

test("detection leaves keyed tracks alone unless replacing is on", () => {
  const list = [t(1, ""), t(2, "Am"), t(3, "  "), t(4, "8A")];
  const off = detectionPlan(list, { overwrite: false, minConfidencePct: 0 });
  assert.deepEqual(off.targets.map((x) => x.track_id), [1, 3]);
  assert.deepEqual(off.kept.map((x) => x.track_id), [2, 4]);
  const on = detectionPlan(list, { overwrite: true, minConfidencePct: 0 });
  assert.deepEqual(on.targets.map((x) => x.track_id), [1, 2, 3, 4]);
  assert.equal(on.kept.length, 0);
});

test("each confirmation line says what happens to that track's key", () => {
  const s = { overwrite: true, minConfidencePct: 40 };
  assert.equal(detectLine(t(2, "Am"), s), "Track 2: key Am is replaced by the detected key if the result reaches 40% confidence");
  assert.equal(detectLine(t(1, ""), { overwrite: false, minConfidencePct: 0 }), "Track 1: no key yet, the detected key is written");
  // an untitled track is named by its file
  assert.match(detectLine({ ...t(5, ""), title: "" }, DEFAULT_KEY_SETTINGS), /^f5\.mp3: /);
});
