// Section-aware geometry for auto transitions. The backend (`mixAutoTransition`) starts a
// transition on the incoming clip's first beat after its cue and runs it for the length asked.
// With the setting "Start / end on a section boundary" the window is moved so that its start,
// its end, or both, fall on the nearest boundary between two detected sections (an outro or a
// breakdown beginning on the outgoing clip, a drop beginning on the incoming one). Sections are
// app-side estimates (state/sections.svelte.ts) and the positions are placed on the timeline
// through each clip's tempo curve (`clipSectionPoints` in mixedit.ts), so a stretched clip's
// boundaries land where it is heard.
//
// With the setting off, or when neither clip has sections near enough, nothing changes: the
// result is the very suggestion the backend gave. Pure: runs under plain node.
import type { Clip, MixDoc, TransitionSuggestion } from "../api/mixes";
import { SR, clipOf, clipSectionPoints, type SectionPoint, type SectionsLookup } from "./mixedit.ts";
import type { RecipePrefs, SectionEdge } from "./transitionRecipes.ts";

const BEATS_PER_BAR = 4;

/** Words for a kind in a note ("a drop begins"). */
const KIND_WORD: Record<string, string> = { intro: "an intro", build: "a build", drop: "a drop", breakdown: "a breakdown", outro: "an outro", section: "a new section" };

/** "2 bars", "1 bar", "half a bar", "3 beats": a distance in samples, in bars of `bar` samples. */
export function barsText(samples: number, bar: number): string {
  const bars = Math.abs(samples) / Math.max(1, bar);
  const beats = Math.round(bars * BEATS_PER_BAR);
  if (beats === 0) return "no distance";
  if (beats % BEATS_PER_BAR === 0) {
    const n = beats / BEATS_PER_BAR;
    return `${n} bar${n === 1 ? "" : "s"}`;
  }
  if (beats < BEATS_PER_BAR) return `${beats} beat${beats === 1 ? "" : "s"}`;
  return `${(beats / BEATS_PER_BAR).toFixed(1)} bars`;
}

export type AlignInput = {
  mode: SectionEdge;
  /** Section boundaries of the two clips on the timeline (samples). */
  points: readonly SectionPoint[];
  /** The earliest the transition may start: the incoming clip's audible start. */
  lo: number;
  /** The latest it may end: the earlier of the two clips' audible ends. */
  hi: number;
  /** The shortest transition worth keeping, samples (the recipe's own minimum). */
  minLength: number;
  /** How far a boundary may be from where the transition would otherwise start or end, in bars. */
  reachBars: number;
};

export type Aligned = {
  suggestion: TransitionSuggestion;
  /** The window differs from the backend's, so its stock tools no longer fit it. */
  moved: boolean;
  /** Sentences for the user: what moved, or why nothing did. */
  notes: string[];
};

const nearest = (points: readonly SectionPoint[], target: number, reach: number, ok: (p: SectionPoint) => boolean): SectionPoint | null => {
  let best: SectionPoint | null = null;
  for (const p of points) {
    if (!ok(p) || Math.abs(p.time - target) > reach + 0.5) continue;
    // a tie goes to the earlier boundary (the points are ascending)
    if (best === null || Math.abs(p.time - target) < Math.abs(best.time - target) - 1e-9) best = p;
  }
  return best;
};

/** Moves the window of `base` onto section boundaries as `o.mode` says. Returns `base` itself
 * (`moved` false) when the mode is "off", there are no boundaries, or none is within reach. */
export function alignTransition(base: TransitionSuggestion, o: AlignInput): Aligned {
  const same: Aligned = { suggestion: base, moved: false, notes: [] };
  const unmoved = (notes: string[]): Aligned => ({ suggestion: notes.length ? { ...base, notes: [...base.notes, ...notes] } : base, moved: false, notes });
  const t = base.transition;
  if (o.mode === "off" || !t.between || o.points.length === 0) return same;
  const beat = base.beat;
  if (!(beat > 0)) return same;
  const bar = beat * BEATS_PER_BAR;
  const reach = Math.max(0, o.reachBars) * bar;
  const s0 = t.start;
  const e0 = t.start + t.length;
  const moveStart = o.mode === "start" || o.mode === "both";
  const moveEnd = o.mode === "end" || o.mode === "both";
  const notes: string[] = [];

  let s = s0;
  let startPick: SectionPoint | null = null;
  if (moveStart) {
    startPick = nearest(o.points, s0, reach, (p) => p.time >= o.lo && p.time <= o.hi - o.minLength);
    if (startPick) s = Math.round(startPick.time);
    else notes.push(`No section boundary within ${o.reachBars} bar${o.reachBars === 1 ? "" : "s"} of the start, so it keeps the beat-matched start.`);
  }

  // the length is kept unless the end is moved onto a boundary
  let e = s + t.length;
  let endPick: SectionPoint | null = null;
  if (moveEnd) {
    endPick = nearest(o.points, e0, reach, (p) => p.time >= s + o.minLength && p.time <= o.hi);
    if (endPick) e = Math.round(endPick.time);
    else notes.push(`No section boundary within ${o.reachBars} bar${o.reachBars === 1 ? "" : "s"} of the end, so it keeps its length.`);
  }
  // a moved start keeps the length when it still fits, else the whole beats that do
  if (e > o.hi + 1) {
    const room = o.hi - s;
    const whole = Math.floor((room + 1) / beat) * beat;
    e = s + (whole >= o.minLength - 1 ? whole : room);
    notes.push(`Only ${barsText(room, bar)} of both clips are left after the new start: the transition is shortened to fit.`);
  }

  if (!startPick && !endPick) return unmoved(notes);

  const length = Math.max(1, Math.round(e - s));
  if (startPick) {
    const d = s - s0;
    notes.push(`Starts where ${KIND_WORD[startPick.after] ?? "a new section"} begins${Math.abs(d) < 1 ? "" : ` (${barsText(d, bar)} ${d < 0 ? "earlier" : "later"} than the beat-matched start)`}.`);
  }
  if (endPick) {
    const d = s + length - e0;
    notes.push(`Ends where ${KIND_WORD[endPick.after] ?? "a new section"} begins${Math.abs(d) < 1 ? "" : ` (${barsText(d, bar)} ${d < 0 ? "shorter" : "longer"} than asked)`}.`);
  }

  const suggestion: TransitionSuggestion = {
    ...base,
    transition: { ...t, start: s, length, startSec: s / SR, lengthSec: length / SR },
    bars: length / bar,
    barsAvailable: Math.max(0, (o.hi - s) / bar),
    notes: [...base.notes, ...notes],
  };
  return { suggestion, moved: true, notes };
}

/** The `AlignInput` for the two clips a suggestion joins, from the section lookup and the
 * settings; `minLength` is the shortest the recipe in use accepts, in samples. */
export function alignInputFor(doc: MixDoc, base: TransitionSuggestion, prefs: Pick<RecipePrefs, "sectionEdge" | "sectionReachBars">, lookup: SectionsLookup | undefined, minLength: number): AlignInput | null {
  const between = base.transition.between;
  if (prefs.sectionEdge === "off" || !lookup || !between) return null;
  const from = clipOf(doc, between.from);
  const to = clipOf(doc, between.to);
  if (!from || !to) return null;
  const lo = to.visibleStart;
  const hi = Math.min(from.visibleEnd, to.visibleEnd);
  const inside = (c: Clip) => clipSectionPoints(c, lookup(c.trackId)).filter((p) => p.time >= lo && p.time <= hi);
  return {
    mode: prefs.sectionEdge,
    points: [...inside(from), ...inside(to)].sort((a, b) => a.time - b.time),
    lo,
    hi,
    minLength,
    reachBars: prefs.sectionReachBars,
  };
}

/** One call for both callers (the mix editor's Auto buttons and the set builder's mix creation):
 * the suggestion aligned per the settings, or untouched. */
export function alignSuggestion(doc: MixDoc, base: TransitionSuggestion, prefs: Pick<RecipePrefs, "sectionEdge" | "sectionReachBars">, lookup: SectionsLookup | undefined, minLength: number): Aligned {
  const input = alignInputFor(doc, base, prefs, lookup, minLength);
  return input ? alignTransition(base, input) : { suggestion: base, moved: false, notes: [] };
}
