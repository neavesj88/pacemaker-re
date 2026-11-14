// Matching a clip's tempo to a target tempo, half and double time included. Pure (no imports
// from the app), so it runs under plain node (scripts/test-mixedit.mjs).
//
// A 130 BPM track joining a mix at 87 BPM (= 174 in double time) must not be slowed to 87: that is
// a 33 % stretch that sounds wrong. The ratio nearest to 1.0 among "the target", "twice the
// target" and "half the target" is the one to play at; and when even that one stretches the audio
// by more than the threshold, the caller must not apply it silently.

/** The factors of the target tempo that count as the same groove: half time, the tempo itself, double time. */
export const FACTORS = [1, 2, 0.5] as const;
export type Factor = (typeof FACTORS)[number];

/** Default for the setting "stretch above which a match is not applied silently", per cent. */
export const DEFAULT_STRETCH_WARN_PCT = 8;

export type TempoChoice = {
  /** The tempo the clip should play at: the target times `factor`. */
  playsAt: number;
  /** Playback speed relative to the track's own tempo (playsAt / bpm). */
  speed: number;
  /** 1, 2 or 0.5 times the target. */
  factor: Factor;
  /** (speed - 1) * 100: +23 means 23 % faster. */
  stretchPct: number;
};

/** Per cent change a speed ratio is. */
export const stretchPct = (speed: number) => (speed - 1) * 100;

/** Text such as "+23 %" or "-4.5 %" for a stretch. */
export function stretchText(pct: number): string {
  const a = Math.abs(pct);
  const t = a >= 10 ? a.toFixed(0) : a.toFixed(1);
  return `${pct >= 0 ? "+" : "-"}${t} %`;
}

/** True when a speed changes the audio by more than `thresholdPct` per cent either way. */
export const exceeds = (speed: number, thresholdPct: number) => Math.abs(speed - 1) * 100 > thresholdPct + 1e-9;

/** Picks the tempo to play a track of `bpm` at so it matches `target`: the closest to the track's
 * own tempo (in ratio terms) of the target, double the target and half the target. A tie goes to
 * the target itself. Null when either tempo is not a positive number. */
export function chooseTempo(bpm: number, target: number): TempoChoice | null {
  if (!(bpm > 0) || !(target > 0) || !Number.isFinite(bpm) || !Number.isFinite(target)) return null;
  let best: TempoChoice | null = null;
  let bestDist = Infinity;
  for (const factor of FACTORS) {
    const playsAt = target * factor;
    const speed = playsAt / bpm;
    const dist = Math.abs(Math.log(speed));
    if (dist < bestDist - 1e-12) {
      bestDist = dist;
      best = { playsAt, speed, factor, stretchPct: stretchPct(speed) };
    }
  }
  return best;
}

/** Pitch change of a speed ratio in per cent for pitch (vinyl) mode, and the same in semitones. */
export const pitchSemitones = (speed: number) => 12 * Math.log2(speed > 0 ? speed : 1);
