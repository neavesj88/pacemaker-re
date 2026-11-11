// Run: node --experimental-strip-types --test src/lib/views/grid/vtempoLogic.test.mjs
import assert from "node:assert/strict";
import { test } from "node:test";
import {
  addTap,
  biasNote,
  DEFAULT_VTEMPO,
  dragLimits,
  driftClass,
  evenRange,
  fmtClock,
  gapBpms,
  gapLimits,
  hitBeat,
  normaliseVTempo,
  sectionLines,
  sectionOfBeat,
  sectionRange,
  spanLine,
  tapBpm,
  validateVTempo,
} from "./vtempoLogic.ts";

const SR = 44100;

test("the defaults are valid and a stored value overrides only what has the right type", () => {
  assert.equal(validateVTempo(DEFAULT_VTEMPO), "");
  const s = normaliseVTempo({ drift_threshold_ms: 35, tap_count: "many", auto_check: true, unknown: 1, section_min_beats: NaN });
  assert.equal(s.drift_threshold_ms, 35);
  assert.equal(s.tap_count, DEFAULT_VTEMPO.tap_count, "wrong type: default");
  assert.equal(s.auto_check, true);
  assert.equal(s.section_min_beats, DEFAULT_VTEMPO.section_min_beats, "NaN: default");
  assert.ok(!("unknown" in s));
  assert.deepEqual(normaliseVTempo(null), DEFAULT_VTEMPO);
  assert.deepEqual(normaliseVTempo("junk"), DEFAULT_VTEMPO);
});

test("every setting is range checked", () => {
  const bad = (patch) => validateVTempo({ ...DEFAULT_VTEMPO, ...patch });
  assert.match(bad({ section_tolerance_pct: -1 }), /tolerance/);
  assert.match(bad({ section_min_beats: 0 }), /section/);
  assert.match(bad({ section_min_beats: 2.5 }), /section/);
  assert.match(bad({ drift_window_pct: 80 }), /window/);
  assert.match(bad({ drift_window_max_ms: 1 }), /window/);
  assert.match(bad({ drift_threshold_ms: 0 }), /threshold/);
  assert.match(bad({ drift_min_run: 0 }), /span/);
  assert.match(bad({ drift_max_gap: -1 }), /bridge/);
  assert.match(bad({ drift_max_gap: 1.5 }), /bridge/);
  assert.match(bad({ onset_min_strength: 0 }), /onset/);
  assert.match(bad({ grab_px: 1 }), /grab/);
  assert.match(bad({ section_list_max: 5 }), /section list/);
  assert.match(bad({ follow_smooth_beats: 0 }), /Smoothing/);
  assert.match(bad({ tap_count: 1 }), /Tap/);
  assert.match(bad({ tap_reset_secs: 0 }), /pause/);
  assert.match(bad({ drift_threshold_ms: NaN }), /threshold/);
});

test("tap tempo needs two taps and averages the interval", () => {
  assert.equal(tapBpm([]), null);
  assert.equal(tapBpm([1000]), null);
  assert.equal(tapBpm([1000, 1500]), 120);
  // five taps 468.75 ms apart = 128 BPM, whatever the start time
  const taps = [0, 1, 2, 3, 4].map((k) => 12345 + k * 468.75);
  assert.ok(Math.abs(tapBpm(taps) - 128) < 1e-9);
  assert.equal(tapBpm([5, 5]), null, "simultaneous taps are not a tempo");
});

test("taps accumulate, keep the last N and restart after a pause", () => {
  let t = [];
  for (let k = 0; k < 5; k++) t = addTap(t, k * 500, 2.5, 3);
  assert.deepEqual(t, [1000, 1500, 2000], "only the last three");
  assert.equal(tapBpm(t), 120);
  t = addTap(t, 2000 + 2501, 2.5, 3);
  assert.deepEqual(t, [4501], "a pause longer than the reset starts over");
  t = addTap(t, 4501 + 2500, 2.5, 3);
  assert.equal(t.length, 2, "exactly the reset time still counts");
  assert.deepEqual(addTap([5000], 4000, 2.5, 8), [4000], "a clock that went backwards restarts");
  assert.equal(addTap([1, 2, 3], 4, 2.5, 1).length, 2, "at least two taps are always kept");
});

test("gap limits are the device's 400 and 20 BPM", () => {
  assert.deepEqual(gapLimits(SR), [6615, 132300]);
});

test("a dragged beat stays clear of its neighbours and inside the audio", () => {
  const beats = [1000, 23050, 45100, 67150];
  const [lo, hi] = dragLimits(beats, 1, SR, 100000);
  assert.equal(lo, 1000 + 6615);
  assert.equal(hi, 45100 - 6615);
  // the first beat has no lower neighbour: limited by 0 and the next beat's gap
  assert.deepEqual(dragLimits(beats, 0, SR, 100000), [0, 23050 - 6615]);
  // the last beat is limited by the end of the audio and by the longest gap
  const [l2, h2] = dragLimits(beats, 3, SR, 70000);
  assert.equal(l2, 45100 + 6615);
  assert.equal(h2, 70000);
  assert.equal(dragLimits(beats, 3, SR, null)[1], 45100 + 132300);
  // a middle beat cannot go so far that the gap after it gets slower than 20 BPM
  const wide = [0, 100000, 200000];
  const [lw, hw] = dragLimits(wide, 1, SR, null);
  assert.equal(lw, 200000 - 132300);
  assert.equal(hw, 132300, "the longest gap after the first beat is the tighter limit here");
});

test("hitBeat finds the nearest line within the pixel tolerance", () => {
  const secs = [0.5, 1.0, 1.5, 2.0];
  assert.equal(hitBeat(secs, 1.002, 100, 6), 1);
  assert.equal(hitBeat(secs, 1.2, 100, 6), -1, "20 px away");
  assert.equal(hitBeat(secs, 1.24, 100, 30), 1, "nearest of two");
  assert.equal(hitBeat(secs, 1.26, 100, 30), 2, "nearest of two");
  assert.equal(hitBeat(secs, 0.1, 100, 50), 0, "before the first");
  assert.equal(hitBeat(secs, 9, 100, 50), -1, "after the last, out of reach");
  assert.equal(hitBeat([], 1, 100, 6), -1);
  assert.equal(hitBeat(secs, 1, 0, 6), -1, "no zoom");
  assert.equal(hitBeat(new Float64Array(secs), 1.499, 1000, 6), 2);
});

test("gap tempi come from the positions, the last beat from the gap before it", () => {
  const b = gapBpms([0, 0.5, 1.0, 1.4]);
  assert.deepEqual(b.slice(0, 2), [120, 120]);
  assert.ok(Math.abs(b[2] - 150) < 1e-9);
  assert.ok(Math.abs(b[3] - 150) < 1e-9);
  assert.deepEqual(gapBpms([3]), [0]);
  assert.deepEqual(gapBpms([]), []);
});

test("section ranges: the section's own beats up to the next section, the last one to the end", () => {
  const secs = [
    { first_beat: 0, end_beat: 32 },
    { first_beat: 32, end_beat: 64 },
  ];
  assert.deepEqual(sectionRange(secs[0], 64), { from_beat: 0, to_beat: 32 });
  assert.deepEqual(sectionRange(secs[1], 64), { from_beat: 32, to_beat: null });
  assert.deepEqual(evenRange(secs[0], 64), { from_beat: 0, to_beat: 32 });
  assert.deepEqual(evenRange(secs[1], 64), { from_beat: 32, to_beat: 63 });
  assert.equal(evenRange({ first_beat: 10, end_beat: 11 }, 64), null, "nothing between the anchors");
  assert.equal(evenRange({ first_beat: 62, end_beat: 64 }, 64), null);
  assert.equal(sectionOfBeat(secs, 31), 0);
  assert.equal(sectionOfBeat(secs, 32), 1);
  assert.equal(sectionOfBeat(secs, 64), -1);
});

test("section and drift lines name every number", () => {
  assert.equal(fmtClock(72.34), "1:12.3");
  assert.equal(fmtClock(5), "0:05.0");
  const lines = sectionLines(
    [
      { first_beat: 0, end_beat: 32, start_frame: 22050, bpm: 120, min_bpm: 120, max_bpm: 120, beats: 32 },
      { first_beat: 32, end_beat: 40, start_frame: SR * 60, bpm: 126.5, min_bpm: 126, max_bpm: 127, beats: 8 },
    ],
    SR,
  );
  assert.equal(lines[0], "Section 1: beats 1 to 32 from 0:00.5, 120.00 BPM");
  assert.equal(lines[1], "Section 2: beats 33 to 40 from 1:00.0, 126.50 BPM (126.00 to 127.00)");
  const span = { first_beat: 78, last_beat: 95, start_frame: SR * 72, end_frame: SR * 90, off_beats: 18, mean_ms: 36.4, max_abs_ms: 47.6 };
  assert.equal(spanLine(span, SR), "Beats 79 to 96, 1:12.0 to 1:30.0: lines 36 ms early on average, 48 ms at worst");
  assert.match(spanLine({ ...span, mean_ms: -12 }, SR), /12 ms late/);
});

test("offsets are classed against the median, not against zero", () => {
  assert.equal(driftClass(null, 0, 20), "none");
  assert.equal(driftClass(25, 0, 20), "off");
  assert.equal(driftClass(25, 20, 20), "ok", "a grid that is 20 ms late as a whole is not drifting");
  assert.equal(driftClass(-5, 0, 20), "ok");
  assert.equal(driftClass(-45, 0, 20), "off");
  assert.equal(biasNote(0.4), null);
  assert.match(biasNote(12.34), /12\.3 ms early/);
  assert.match(biasNote(-8), /8\.0 ms late/);
});
