// Pure helpers for the grid editor's drawing and metronome: no DOM, no state.

export const clamp = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v));

/** `m:ss.mmm` */
export function fmtTime(secs: number): string {
  const s = Math.max(0, secs);
  const m = Math.floor(s / 60);
  const rest = s - m * 60;
  return `${m}:${rest.toFixed(3).padStart(6, "0")}`;
}

/** Index of the first element of the ascending array that is >= x (arr.length if none). */
export function lowerBound(arr: ArrayLike<number>, x: number): number {
  let lo = 0;
  let hi = arr.length;
  while (lo < hi) {
    const mid = (lo + hi) >>> 1;
    if (arr[mid] < x) lo = mid + 1;
    else hi = mid;
  }
  return lo;
}

/** Is beat `i` a bar line, given the index of the first one? Beats before it are pre-roll. */
export function isDownbeat(i: number, firstDownbeat: number | null): boolean {
  return firstDownbeat !== null && i >= firstDownbeat && (i - firstDownbeat) % 4 === 0;
}

/** Beat indexes whose time lies in (after, upTo]: what the metronome still has to schedule. */
export function beatsToSchedule(beatSecs: ArrayLike<number>, after: number, upTo: number): number[] {
  const out: number[] = [];
  for (let i = lowerBound(beatSecs, after); i < beatSecs.length && beatSecs[i] <= upTo; i++) {
    if (beatSecs[i] > after) out.push(i);
  }
  return out;
}

/** Largest value per band over the columns that cover the fraction range [f0, f1) of the
 *  track (at least one column), so a zoomed-out view keeps its transients. */
export function bandsOver(peaks: ArrayLike<[number, number, number]>, f0: number, f1: number): [number, number, number] {
  const n = peaks.length;
  if (n === 0) return [0, 0, 0];
  const a = clamp(Math.floor(f0 * n), 0, n - 1);
  const b = clamp(Math.max(a + 1, Math.ceil(f1 * n)), a + 1, n);
  let lo = 0;
  let mid = 0;
  let hi = 0;
  for (let i = a; i < b; i++) {
    const p = peaks[i];
    if (p[0] > lo) lo = p[0];
    if (p[1] > mid) mid = p[1];
    if (p[2] > hi) hi = p[2];
  }
  return [lo, mid, hi];
}

/** A round step (1, 2, 5 x 10^n seconds) giving ticks at least `minPx` apart. */
export function tickStep(pxPerSec: number, minPx: number): number {
  const raw = minPx / pxPerSec;
  const mag = 10 ** Math.floor(Math.log10(raw));
  for (const m of [1, 2, 5, 10]) if (m * mag >= raw) return m * mag;
  return 10 * mag;
}

/** Seconds of the first bar line: the beat the grid marks as the downbeat, not simply the first
 *  beat. `null` when the file marks none or points past its beats. */
export function firstDownbeatSecs(
  doc: { beats: ArrayLike<{ pos: number }>; first_downbeat: number | null } | null,
  sampleRate: number,
): number | null {
  if (!doc || doc.first_downbeat === null || sampleRate <= 0) return null;
  const b = doc.beats[doc.first_downbeat];
  return b ? b.pos / sampleRate : null;
}
