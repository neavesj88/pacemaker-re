// Run: node --experimental-strip-types --test src/lib/state/batchLogic.test.mjs
import assert from "node:assert/strict";
import { test } from "node:test";
import {
  changing,
  counts,
  DEFAULT_SUSPECT_SETTINGS,
  firstShiftMs,
  fmtFrames,
  fmtShift,
  formatGenreWindows,
  mixesWarning,
  mixUse,
  normaliseSuspect,
  opDescription,
  opTitle,
  parseGenreWindows,
  parseNudge,
  previewKey,
  rowLine,
  suspectKinds,
  validateSettings,
} from "./batchLogic.ts";

const SR = 44100;
const row = (o = {}) => ({
  track_id: 1,
  label: "A - One",
  op: "halve",
  before_bpm: 172,
  after_bpm: 86,
  before_first: 3000,
  after_first: 3000,
  before_beats: 200,
  after_beats: 100,
  sample_rate: SR,
  skip: null,
  note: null,
  mixes: [],
  ...o,
});

test("the default rules are valid and a damaged stored value falls back field by field", () => {
  assert.equal(validateSettings(DEFAULT_SUSPECT_SETTINGS), "");
  const d = normaliseSuspect(null);
  assert.deepEqual(d.rules, DEFAULT_SUSPECT_SETTINGS);
  assert.equal(d.auto, true);
  const odd = normaliseSuspect({
    auto: "yes",
    rules: { tempo_rule: "no", fallback_min: "x", fallback_max: 200, genres: [{ name: "a", min: 1, max: 2 }, { name: " ", min: 1, max: 2 }, null, { name: "b", min: "z", max: 3 }] },
  });
  assert.equal(odd.auto, true);
  assert.equal(odd.rules.tempo_rule, true);
  assert.equal(odd.rules.fallback_min, 70);
  assert.equal(odd.rules.fallback_max, 200);
  assert.deepEqual(odd.rules.genres, [{ name: "a", min: 1, max: 2 }], "unusable windows are dropped");
  const kept = normaliseSuspect({ auto: false, rules: { genres: [], min_confidence_pct: 30 } });
  assert.equal(kept.auto, false);
  assert.deepEqual(kept.rules.genres, []);
  assert.equal(kept.rules.min_confidence_pct, 30);
});

test("the rules are checked the way the backend checks them", () => {
  const ok = DEFAULT_SUSPECT_SETTINGS;
  assert.match(validateSettings({ ...ok, fallback_min: 100, fallback_max: 100 }), /general tempo window/);
  assert.match(validateSettings({ ...ok, fallback_min: 10 }), /from 20 up to 400/);
  assert.match(validateSettings({ ...ok, fallback_max: 401 }), /from 20 up to 400/);
  assert.match(validateSettings({ ...ok, genres: [{ name: " ", min: 100, max: 120 }] }), /needs a name/);
  assert.match(validateSettings({ ...ok, genres: [{ name: "x", min: 130, max: 120 }] }), /“x”/);
  assert.match(validateSettings({ ...ok, min_confidence_pct: 101 }), /confidence floor/);
  assert.match(validateSettings({ ...ok, steady_tolerance_pct: 0 }), /steady-gap/);
  assert.match(validateSettings({ ...ok, coverage_target_pct: 5 }), /coverage target/);
});

test("genre windows round-trip through their text form and bad lines are named", () => {
  const text = formatGenreWindows(DEFAULT_SUSPECT_SETTINGS.genres);
  assert.match(text, /^house 115-132$/m);
  assert.match(text, /^drum & bass 160-180$/m);
  assert.deepEqual(parseGenreWindows(text), { genres: DEFAULT_SUSPECT_SETTINGS.genres });
  assert.deepEqual(parseGenreWindows("  Deep House: 118 – 124.5 \n\n psy-trance 138 to 150").genres, [
    { name: "Deep House", min: 118, max: 124.5 },
    { name: "psy-trance", min: 138, max: 150 },
  ]);
  assert.deepEqual(parseGenreWindows("").genres, []);
  assert.match(parseGenreWindows("house 115-132\nnonsense").error, /Line 2 .*nonsense/);
  assert.match(parseGenreWindows("house 115-132\nHouse 100-120").error, /listed twice/);
});

test("a typed nudge is read with a comma or a point and kept inside the limit", () => {
  assert.deepEqual(parseNudge("-12,5"), { ms: -12.5 });
  assert.deepEqual(parseNudge(" 20 "), { ms: 20 });
  assert.deepEqual(parseNudge("5000"), { ms: 5000 });
  for (const bad of ["", " ", "abc", "0", "-0", "5000.1", "-6000", "NaN", "Infinity", "1 2"]) {
    assert.ok("error" in parseNudge(bad), `${JSON.stringify(bad)} must be refused`);
  }
});

test("positions and shifts are worded in milliseconds and m:ss.mmm", () => {
  assert.equal(fmtFrames(44100 * 65 + 441 * 12, SR), "1:05.120");
  assert.equal(fmtFrames(0, SR), "0:00.000");
  assert.equal(fmtFrames(null, SR), "?");
  assert.equal(fmtFrames(100, 0), "?");
  assert.equal(firstShiftMs(row({ after_first: 3882 })), 20);
  assert.equal(firstShiftMs(row({ before_first: null })), null);
  assert.equal(fmtShift(20), "+20.0 ms");
  assert.equal(fmtShift(-7.25), "−7.3 ms");
  assert.equal(fmtShift(null), "?");
});

test("each row says in words what happens to the track", () => {
  assert.equal(rowLine(row()), "A - One: halve, 172.00 → 86.00 BPM, first beat 0:00.068 → 0:00.068 (+0.0 ms)");
  assert.equal(rowLine(row({ op: "nudge", after_bpm: 172, after_first: 3882 })), "A - One: nudge, 172.00 → 172.00 BPM, first beat 0:00.068 → 0:00.088 (+20.0 ms)");
  assert.equal(rowLine(row({ skip: "the grid is already like that", after_bpm: null })), "A - One: left as it is (the grid is already like that)");
  assert.equal(rowLine(row(), false), "A - One: left as it is (not ticked)");
});

test("the mixes warning counts each mix once, over the ticked tracks that change", () => {
  const m1 = { id: 1, name: "Mix A" };
  const m2 = { id: 2, name: "Mix B" };
  const rows = [
    row({ track_id: 1, mixes: [m1, m2] }),
    row({ track_id: 2, mixes: [m1] }),
    row({ track_id: 3, mixes: [m2], skip: "the grid is already like that" }),
    row({ track_id: 4, mixes: [{ id: 3, name: "Mix C" }] }),
    row({ track_id: 5, mixes: [{ id: -1, name: "(could not check: x)" }] }),
  ];
  const use = mixUse(rows, new Set([1, 2, 3, 5]));
  assert.deepEqual(
    use.mixes,
    [
      { id: 1, name: "Mix A", tracks: 2 },
      { id: 2, name: "Mix B", tracks: 1 },
    ],
    "skipped track 3 and unticked track 4 are not counted",
  );
  assert.equal(use.unchecked, 1);
  const text = mixesWarning(use);
  assert.match(text, /2 mixes use some of these tracks: Mix A \(2 tracks\), Mix B \(1 track\)\./);
  assert.match(text, /drift out of step until beatmatched again/);
  assert.match(text, /Could not check which mixes use one of the tracks/);
  assert.equal(mixesWarning({ mixes: [], unchecked: 0 }), "");
  assert.match(mixesWarning({ mixes: [{ id: 1, name: "Solo", tracks: 1 }], unchecked: 0 }), /^1 mix uses .*that mix may drift/);
});

test("the counts separate what changes, what is left alone and what the user unticked", () => {
  const rows = [row({ track_id: 1 }), row({ track_id: 2 }), row({ track_id: 3, skip: "x" })];
  assert.deepEqual(counts(rows, new Set([1])), { change: 1, skipped: 1, unticked: 1 });
  assert.deepEqual(changing(rows).map((r) => r.track_id), [1, 2]);
});

test("a preview belongs to the operation and tracks it was made for", () => {
  assert.equal(previewKey("halve", 5, [3, 1, 2]), previewKey("halve", 99, [1, 2, 3]), "the nudge amount only matters for a nudge");
  assert.notEqual(previewKey("nudge", 5, [1]), previewKey("nudge", 6, [1]));
  assert.notEqual(previewKey("halve", 0, [1]), previewKey("double", 0, [1]));
  assert.notEqual(previewKey("halve", 0, [1]), previewKey("halve", 0, [1, 2]));
});

test("the wording names the operation and says BPM and grid change together", () => {
  assert.equal(opTitle("halve", 3, "P:\\"), "Halve the BPM of 3 tracks on P:\\ ?");
  assert.equal(opTitle("nudge", 1, "/mnt/pm", -12.5), "Move the beat lines of 1 track by −12.5 ms on /mnt/pm ?");
  assert.match(opTitle("suggested", 2, "P:\\"), /suggested half \/ double fix to 2 tracks/);
  assert.match(opTitle("reanalyse", 1, "P:\\"), /new analysis/);
  for (const k of ["nudge", "halve", "double", "suggested", "reanalyse"]) assert.match(opDescription(k), /rewritten together/);
  assert.match(opDescription("reanalyse"), /set by hand is replaced/);
});

test("the filter explains a suspect in a few words", () => {
  assert.equal(suspectKinds({ tempo_flag: true, quality_flag: false, fix: "halve" }), "tempo too high, half fits");
  assert.equal(suspectKinds({ tempo_flag: true, quality_flag: true, fix: "double" }), "tempo too low, double fits; low grid confidence");
  assert.equal(suspectKinds({ tempo_flag: true, quality_flag: false, fix: null }), "tempo outside window");
  assert.equal(suspectKinds({ tempo_flag: false, quality_flag: true, fix: null }), "low grid confidence");
});
