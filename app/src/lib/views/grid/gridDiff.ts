// Pure comparison of an old and a new beat grid, for the batch preview's diff view: how the beat
// lines moved, which were added or dropped, and how the tempo and bar lines changed. No DOM or
// state, so it runs under plain node (gridDiff.test.mjs).
import type { GridDoc } from "../../api/grid";

export type DiffStats = {
  beatsBefore: number;
  beatsAfter: number;
  bpmBefore: number | null;
  bpmAfter: number;
  /** First beat line in seconds. */
  firstBefore: number | null;
  firstAfter: number;
  /** Seconds of the first bar line (the first downbeat), when the grid marks one. */
  barBefore: number | null;
  barAfter: number | null;
  /** Beat lines present in both grids (within a quarter of a beat) that did not move at all. */
  same: number;
  /** Present in both but at a different time. */
  moved: number;
  /** Only in the new grid / only in the old grid. */
  added: number;
  removed: number;
  /** Of the beat lines present in both: average and largest shift in milliseconds. */
  meanMoveMs: number;
  maxMoveMs: number;
  /** When every beat line of the new grid is an old one moved by the same time, that time in
   *  milliseconds (positive = later, measured to the nearest old line); otherwise null. */
  uniformShiftMs: number | null;
};

const median = (v: number[]) => {
  const s = [...v].sort((a, b) => a - b);
  return s[Math.floor(s.length / 2)] ?? 0;
};

const gaps = (pos: number[]) => pos.slice(1).map((p, i) => p - pos[i]);

/** Median local tempo of a grid: what the device stores as the track's BPM. */
export const gridBpm = (doc: GridDoc) => median(doc.beats.map((b) => b.bpm));

function barTime(doc: GridDoc | null, sr: number): number | null {
  const i = doc?.first_downbeat;
  return doc && i !== null && i !== undefined && doc.beats[i] ? doc.beats[i].pos / sr : null;
}

/** A shift that maps (almost) every old beat onto a new one, tried from the first few beats of each
 *  grid (a nudge or a tempo change drops some beats at the start or end). A regular grid looks the
 *  same after any whole number of beats, so of the shifts that fit the one nearest zero is returned:
 *  the distance from each new beat line to the old one it replaces. Null when none fits. */
function uniformShift(a: number[], b: number[]): number | null {
  if (a.length === 0 || b.length === 0) return null;
  const need = Math.floor(0.9 * Math.min(a.length, b.length));
  const candidates = new Set<number>();
  for (let i = 0; i < Math.min(4, a.length); i++) candidates.add(b[0] - a[i]);
  for (let j = 0; j < Math.min(4, b.length); j++) candidates.add(b[j] - a[0]);
  let best: number | null = null;
  for (const shift of candidates) {
    if (best !== null && Math.abs(shift) >= Math.abs(best)) continue;
    let hit = 0;
    let j = 0;
    for (const p of a) {
      const want = p + shift;
      while (j < b.length && b[j] < want - 2) j++;
      if (j < b.length && Math.abs(b[j] - want) <= 2) hit++;
    }
    // the new grid must also have no beat lines that the shift does not explain
    if (hit >= need && Math.abs(b.length - hit) <= Math.max(2, Math.floor(0.1 * b.length))) best = shift;
  }
  return best;
}

/** Compares `before` (null when the track had no readable grid) with `after`. */
export function diffGrids(before: GridDoc | null, after: GridDoc, sampleRate: number): DiffStats {
  const a = before ? before.beats.map((b) => b.pos) : [];
  const b = after.beats.map((x) => x.pos);
  const tol = Math.max(1, 0.25 * Math.min(a.length > 1 ? median(gaps(a)) : Infinity, b.length > 1 ? median(gaps(b)) : Infinity));
  // when one shift explains the new grid, pair the lines through it (a move of 200 ms is far more
  // than a quarter beat on a fast track, but it is still the same line, moved)
  const shift = uniformShift(a, b);
  const offset = shift ?? 0;
  const pairTol = shift === null || shift === 0 ? tol : 2;
  let same = 0;
  let moved = 0;
  let added = 0;
  let removed = 0;
  let sum = 0;
  let max = 0;
  let i = 0;
  let j = 0;
  // one-to-one matching of the two ascending lists
  while (i < a.length && j < b.length) {
    const d = b[j] - (a[i] + offset);
    if (Math.abs(d) <= pairTol) {
      const dist = Math.abs(b[j] - a[i]);
      if (dist === 0) same++;
      else moved++;
      sum += dist;
      max = Math.max(max, dist);
      i++;
      j++;
    } else if (d > 0) {
      removed++;
      i++;
    } else {
      added++;
      j++;
    }
  }
  removed += a.length - i;
  added += b.length - j;
  const matched = same + moved;
  const ms = (frames: number) => (frames / sampleRate) * 1000;
  return {
    beatsBefore: a.length,
    beatsAfter: b.length,
    bpmBefore: before ? gridBpm(before) : null,
    bpmAfter: gridBpm(after),
    firstBefore: a.length ? a[0] / sampleRate : null,
    firstAfter: b[0] / sampleRate,
    barBefore: barTime(before, sampleRate),
    barAfter: barTime(after, sampleRate),
    same,
    moved,
    added,
    removed,
    meanMoveMs: matched ? ms(sum / matched) : 0,
    maxMoveMs: ms(max),
    uniformShiftMs: shift === null ? null : ms(shift),
  };
}

/** The headline of the diff, in words. */
export function diffHeadline(s: DiffStats): string {
  if (s.beatsBefore === 0) return `There was no grid before; the new one has ${s.beatsAfter} beat lines.`;
  if (s.uniformShiftMs !== null && s.uniformShiftMs !== 0 && s.added + s.removed <= 2) {
    return `Every beat line is ${Math.abs(s.uniformShiftMs).toFixed(1)} ms ${s.uniformShiftMs > 0 ? "later" : "earlier"} than the old beat line it replaces.`;
  }
  const parts: string[] = [];
  if (s.same > 0) parts.push(`${s.same} stay`);
  if (s.moved > 0) parts.push(`${s.moved} move (on average ${s.meanMoveMs.toFixed(1)} ms, at most ${s.maxMoveMs.toFixed(1)} ms)`);
  if (s.added > 0) parts.push(`${s.added} added`);
  if (s.removed > 0) parts.push(`${s.removed} dropped`);
  return `Of ${s.beatsBefore} old beat lines, ${parts.join(", ") || "none match"}.`;
}
