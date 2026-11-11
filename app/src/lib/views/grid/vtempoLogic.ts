// Pure logic of the variable-tempo tools: the user's settings (every number has a default), tap
// tempo, the limits of a dragged beat line, hit-testing, and the wording of sections and drift.
// No DOM, no stores, no imports from the app (the node test loads this file on its own).

export type VTempoSettings = {
  /** A beat gap differing from its section's mean by more than this percentage starts a section. */
  section_tolerance_pct: number;
  /** A run of fewer beats than this is folded into its neighbour (unless the user split there). */
  section_min_beats: number;
  /** An onset belongs to a beat when it lies within this percentage of the beat period... */
  drift_window_pct: number;
  /** ...and within this many ms, whichever is smaller. */
  drift_window_max_ms: number;
  /** A beat is off when it differs from the grid's median offset by more than this. */
  drift_threshold_ms: number;
  /** A drift span needs this many consecutive off beats on the same side. */
  drift_min_run: number;
  /** A drift span may bridge this many beats in a row that have no onset near them. */
  drift_max_gap: number;
  /** Onsets weaker than this (standard deviations of the spectral flux) are ignored; applies the next time the audio is checked. */
  onset_min_strength: number;
  /** A beat line can be grabbed within this many pixels. */
  grab_px: number;
  /** The section list shows at most this many sections. */
  section_list_max: number;
  /** "Follow the audio" smooths its corrections over this many beats. */
  follow_smooth_beats: number;
  /** Taps averaged for the tap tempo (the most recent ones). */
  tap_count: number;
  /** A pause longer than this many seconds starts a new tap count. */
  tap_reset_secs: number;
  /** Compare the grid with the audio as soon as a track opens (decodes the file, a few seconds). */
  auto_check: boolean;
};

export const DEFAULT_VTEMPO: VTempoSettings = {
  section_tolerance_pct: 0.5,
  section_min_beats: 4,
  drift_window_pct: 25,
  drift_window_max_ms: 120,
  drift_threshold_ms: 20,
  drift_min_run: 4,
  drift_max_gap: 2,
  onset_min_strength: 1,
  grab_px: 6,
  section_list_max: 200,
  follow_smooth_beats: 5,
  tap_count: 8,
  tap_reset_secs: 2.5,
  auto_check: false,
};

export const VTEMPO_STORAGE = "pacemaker.grid.vtempo.v1";

/** Merges whatever was stored over the defaults, ignoring fields of the wrong type. */
export function normaliseVTempo(raw: unknown): VTempoSettings {
  const out: VTempoSettings = { ...DEFAULT_VTEMPO };
  if (!raw || typeof raw !== "object") return out;
  const src = raw as Record<string, unknown>;
  for (const key of Object.keys(DEFAULT_VTEMPO) as (keyof VTempoSettings)[]) {
    const want = typeof DEFAULT_VTEMPO[key];
    if (typeof src[key] === want && (want !== "number" || Number.isFinite(src[key]))) (out as Record<string, unknown>)[key] = src[key];
  }
  return out;
}

/** Why the settings cannot be used, "" when they can. */
export function validateVTempo(s: VTempoSettings): string {
  const inRange = (v: number, lo: number, hi: number) => Number.isFinite(v) && v >= lo && v <= hi;
  if (!inRange(s.section_tolerance_pct, 0, 50)) return "The section tolerance must be between 0 and 50 %.";
  if (!inRange(s.section_min_beats, 1, 64) || !Number.isInteger(s.section_min_beats)) return "A section must be at least 1 beat (a whole number up to 64).";
  if (!inRange(s.drift_window_pct, 1, 50)) return "The match window must be between 1 and 50 % of a beat.";
  if (!inRange(s.drift_window_max_ms, 2, 500)) return "The longest match window must be between 2 and 500 ms.";
  if (!inRange(s.drift_threshold_ms, 1, 500)) return "The drift threshold must be between 1 and 500 ms.";
  if (!inRange(s.drift_min_run, 1, 64) || !Number.isInteger(s.drift_min_run)) return "A drift span needs at least 1 beat (a whole number up to 64).";
  if (!inRange(s.drift_max_gap, 0, 16) || !Number.isInteger(s.drift_max_gap)) return "A drift span can bridge 0 to 16 beats without an onset.";
  if (!inRange(s.onset_min_strength, 0.1, 20)) return "The onset strength floor must be between 0.1 and 20.";
  if (!inRange(s.grab_px, 2, 40)) return "The grab distance must be between 2 and 40 pixels.";
  if (!inRange(s.section_list_max, 10, 5000) || !Number.isInteger(s.section_list_max)) return "The section list shows 10 to 5000 sections.";
  if (!inRange(s.follow_smooth_beats, 1, 33) || !Number.isInteger(s.follow_smooth_beats)) return "Smoothing is 1 to 33 beats.";
  if (!inRange(s.tap_count, 2, 32) || !Number.isInteger(s.tap_count)) return "Tap tempo averages 2 to 32 taps.";
  if (!inRange(s.tap_reset_secs, 0.5, 30)) return "The tap reset pause must be between 0.5 and 30 seconds.";
  return "";
}

// ---- tap tempo ----

/** The taps (ms timestamps) after one more at `now`: a pause longer than `resetSecs` starts over,
 *  and only the last `maxTaps` are kept. */
export function addTap(taps: readonly number[], now: number, resetSecs: number, maxTaps: number): number[] {
  const last = taps[taps.length - 1];
  const kept = last !== undefined && now - last <= resetSecs * 1000 && now >= last ? [...taps] : [];
  kept.push(now);
  return kept.slice(-Math.max(2, maxTaps));
}

/** Tempo of the taps: 60 s over the mean interval; null until there are two taps. */
export function tapBpm(taps: readonly number[]): number | null {
  if (taps.length < 2) return null;
  const span = taps[taps.length - 1] - taps[0];
  if (!(span > 0)) return null;
  return (60000 * (taps.length - 1)) / span;
}

// ---- beat lines ----

const MIN_BPM = 20;
const MAX_BPM = 400;

/** Shortest and longest gap between two beats in frames: the device's 400 and 20 BPM. */
export function gapLimits(sampleRate: number): [number, number] {
  return [Math.ceil((sampleRate * 60) / MAX_BPM), Math.floor((sampleRate * 60) / MIN_BPM)];
}

/** Frames beat `i` may be dragged to: it stays clear of its neighbours by the shortest gap, within
 *  the longest gap of the one before, and inside the audio. `[lo, hi]`, lo <= hi when movable. */
export function dragLimits(beats: readonly number[], i: number, sampleRate: number, totalFrames: number | null): [number, number] {
  const [minGap, maxGap] = gapLimits(sampleRate);
  let lo = 0;
  let hi = totalFrames ?? Number.MAX_SAFE_INTEGER;
  if (i > 0) {
    lo = Math.max(lo, beats[i - 1] + minGap);
    hi = Math.min(hi, beats[i - 1] + maxGap);
  }
  if (i + 1 < beats.length) {
    hi = Math.min(hi, beats[i + 1] - minGap);
    lo = Math.max(lo, beats[i + 1] - maxGap);
  }
  return [lo, hi];
}

/** Index of the beat nearest `t` (seconds) if it lies within `tolPx` pixels at `pxPerSec`, else -1. */
export function hitBeat(beatSecs: ArrayLike<number>, t: number, pxPerSec: number, tolPx: number): number {
  const n = beatSecs.length;
  if (n === 0 || !(pxPerSec > 0)) return -1;
  let lo = 0;
  let hi = n;
  while (lo < hi) {
    const mid = (lo + hi) >>> 1;
    if (beatSecs[mid] < t) lo = mid + 1;
    else hi = mid;
  }
  let best = -1;
  let bestD = Infinity;
  for (const k of [lo - 1, lo]) {
    if (k < 0 || k >= n) continue;
    const d = Math.abs(beatSecs[k] - t);
    if (d < bestD) {
      bestD = d;
      best = k;
    }
  }
  return best >= 0 && bestD * pxPerSec <= tolPx ? best : -1;
}

/** Tempo implied by the positions: beat `i` takes the gap to the next, the last the gap before it. */
export function gapBpms(beatSecs: ArrayLike<number>): number[] {
  const n = beatSecs.length;
  const out: number[] = [];
  for (let i = 0; i < n; i++) {
    const gap = i + 1 < n ? beatSecs[i + 1] - beatSecs[i] : i > 0 ? beatSecs[i] - beatSecs[i - 1] : 0;
    out.push(gap > 0 ? 60 / gap : 0);
  }
  return out;
}

// ---- sections ----

type SectionLike = { first_beat: number; end_beat: number };

/** What `set_section_bpm` needs for a section: its first beat and the next section's first beat
 *  (null for the last section, which runs to the end of the audio). */
export function sectionRange(s: SectionLike, nBeats: number): { from_beat: number; to_beat: number | null } {
  return { from_beat: s.first_beat, to_beat: s.end_beat < nBeats ? s.end_beat : null };
}

/** The two anchor beats `even` needs for a section, or null when the section has no beat between
 *  them to space. The next section's first beat (or the last beat) is the second anchor. */
export function evenRange(s: SectionLike, nBeats: number): { from_beat: number; to_beat: number } | null {
  const to = s.end_beat < nBeats ? s.end_beat : nBeats - 1;
  return to - s.first_beat >= 2 ? { from_beat: s.first_beat, to_beat: to } : null;
}

/** Index of the section that contains beat `beat`, or -1. */
export function sectionOfBeat(sections: readonly SectionLike[], beat: number): number {
  return sections.findIndex((s) => beat >= s.first_beat && beat < s.end_beat);
}

export function fmtClock(secs: number): string {
  const s = Math.max(0, secs);
  const m = Math.floor(s / 60);
  return `${m}:${(s - m * 60).toFixed(1).padStart(4, "0")}`;
}

/** One line per section for the save confirmation: every section is named, with its beats and tempo. */
export function sectionLines(
  sections: readonly (SectionLike & { start_frame: number; bpm: number; min_bpm: number; max_bpm: number; beats: number })[],
  sampleRate: number,
): string[] {
  return sections.map((s, i) => {
    const spread = s.max_bpm - s.min_bpm > 0.05 ? ` (${s.min_bpm.toFixed(2)} to ${s.max_bpm.toFixed(2)})` : "";
    return `Section ${i + 1}: beats ${s.first_beat + 1} to ${s.first_beat + s.beats} from ${fmtClock(s.start_frame / sampleRate)}, ${s.bpm.toFixed(2)} BPM${spread}`;
  });
}

// ---- drift ----

type SpanLike = { first_beat: number; last_beat: number; start_frame: number; end_frame: number; off_beats: number; mean_ms: number; max_abs_ms: number };

/** "Beats 79 to 96, 1:12.3 to 1:30.0: lines 36 ms early on average, 48 ms at worst". */
export function spanLine(s: SpanLike, sampleRate: number): string {
  const side = s.mean_ms >= 0 ? "early" : "late";
  return `Beats ${s.first_beat + 1} to ${s.last_beat + 1}, ${fmtClock(s.start_frame / sampleRate)} to ${fmtClock(s.end_frame / sampleRate)}: lines ${Math.abs(s.mean_ms).toFixed(0)} ms ${side} on average, ${s.max_abs_ms.toFixed(0)} ms at worst`;
}

/** How a beat's offset is shown: "ok" within the threshold of the median, "off" beyond it, "none" unmatched. */
export function driftClass(offsetMs: number | null, biasMs: number, thresholdMs: number): "none" | "ok" | "off" {
  if (offsetMs === null) return "none";
  return Math.abs(offsetMs - biasMs) > thresholdMs ? "off" : "ok";
}

/** The whole grid sits early (positive) or late: a nudge fixes it. Null when it is within 1 ms. */
export function biasNote(biasMs: number): string | null {
  if (Math.abs(biasMs) < 1) return null;
  return `The whole grid sits ${Math.abs(biasMs).toFixed(1)} ms ${biasMs > 0 ? "early" : "late"} against the audio. That is a nudge, not drift.`;
}
