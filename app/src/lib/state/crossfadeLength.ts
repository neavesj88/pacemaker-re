// Where an automatic crossfade sits and how long it is. The backend (`mixAutoTransition`) starts
// the window on the incoming clip's first beat after its cue and runs it for the length asked. This
// moves it the way a DJ blends: the fade ENDS at or just before the end of the outgoing clip (up to
// a bar early when it snaps to a bar line of the incoming clip; it is gone when the outgoing track
// ends, with no stretch of two tracks playing on after the blend), is the "Auto crossfade length"
// long, never longer than the audio both clips share, a whole number of bars, and starts on
// a bar line of the incoming clip's grid when it has one. The result is a window inside the true
// overlap: `effectiveFade` in mixedit.ts (and the mixer) would not cut it.
//
// Pure: runs under plain node. Used with `alignSuggestion` (sectionTransition.ts), which may then
// move the window on to section boundaries when that setting asks for it.
import type { MixDoc, TransitionSuggestion } from "../api/mixes";
import { BEATS_PER_BAR, SR, clipBeatPoints, clipOf } from "./mixedit.ts";
import type { Aligned } from "./sectionTransition.ts";
import type { CrossfadePlace } from "./transitionRecipes.ts";

/** The length of `x` samples as whole bars of `beat * 4`, or whole beats when under a bar. */
export function wholeLength(x: number, beat: number): number {
  const bar = beat * BEATS_PER_BAR;
  const bars = Math.floor((x + 1) / bar);
  if (bars >= 1) return bars * bar;
  return Math.max(1, Math.floor((x + 1) / beat)) * beat;
}

/** The window of an automatic crossfade for `requestedBars` of length in the audio from `lo` to
 * `end` (both clips are heard together there), a beat being `beat` samples: its length, whole bars
 * (whole beats under a bar), at most what was asked and at most what is shared. The start is the
 * latest line of the incoming clip's grid (its bar lines for a fade of a bar or more, else its
 * beats) that lets the fade end by `end`; without a grid it is `end - length`. */
export function crossfadeWindow(lo: number, end: number, beat: number, requestedBars: number, grid: { bars: readonly number[]; beats: readonly number[] }): { start: number; length: number } | null {
  const room = end - lo;
  if (!(beat > 0) || room < beat * 0.5) return null;
  const length = wholeLength(Math.min(requestedBars * beat * BEATS_PER_BAR, room), beat);
  const latest = end - length;
  const lines = length >= beat * BEATS_PER_BAR ? grid.bars : grid.beats;
  let start = latest;
  // the latest grid line that is not after `latest` and not before `lo`
  for (let i = lines.length - 1; i >= 0; i--) {
    if (lines[i] <= latest + 1 && lines[i] >= lo - 1) {
      start = lines[i];
      break;
    }
  }
  return { start: Math.round(Math.max(lo, start)), length: Math.round(length) };
}

/** `base` moved so the crossfade ends with the outgoing clip (`place` "end"). Returns `base` itself
 * (`moved` false) for "start", when the clips are not known, or when the backend's window is already
 * there. A moved window's stock tools no longer fit, so the caller rebuilds them (`moved`). */
export function placeTransition(doc: MixDoc, base: TransitionSuggestion, requestedBars: number, place: CrossfadePlace): Aligned {
  const same: Aligned = { suggestion: base, moved: false, notes: [] };
  const t = base.transition;
  if (place !== "end" || !t.between || !(requestedBars > 0)) return same;
  const a = clipOf(doc, t.between.from);
  const b = clipOf(doc, t.between.to);
  const beat = base.beat;
  if (!a || !b || !(beat > 0)) return same;
  // not before where the backend put it (the first beat of the incoming clip at or after its cue)
  const lo = Math.max(t.start, b.visibleStart);
  const end = Math.min(a.visibleEnd, b.visibleEnd);
  const bar = beat * BEATS_PER_BAR;
  const pts = clipBeatPoints(b)
    .filter((p) => p.time >= lo - 1 && p.time <= end + 1)
    .sort((x, y) => x.time - y.time);
  const w = crossfadeWindow(lo, end, beat, requestedBars, { bars: pts.filter((p) => p.kind !== "beat").map((p) => p.time), beats: pts.map((p) => p.time) });
  if (!w) return same;
  if (Math.abs(w.start - t.start) <= 1 && Math.abs(w.length - t.length) <= 1) return same;
  const notes: string[] = [];
  const shared = (end - lo) / bar;
  if (w.length / bar < requestedBars - 1e-9) notes.push(`${shared.toFixed(1)} bars of the two clips overlap, so the crossfade is ${w.length / bar} bar${w.length / bar === 1 ? "" : "s"} long`);
  notes.push("The crossfade ends at or just before the end of the outgoing clip");
  const suggestion: TransitionSuggestion = {
    ...base,
    transition: { ...t, start: w.start, length: w.length, startSec: w.start / SR, lengthSec: w.length / SR },
    bars: w.length / bar,
    barsAvailable: Math.max(0, (end - w.start) / bar),
    notes: [...base.notes, ...notes],
  };
  return { suggestion, moved: true, notes };
}
