// Auto-transition recipes for the mix editor. The backend (`mixAutoTransition`) works out the
// geometry from the beat grids (where the overlap starts, how many bars fit, how long a beat
// is); a recipe turns that into the crossfade curve and the tools of the transition. Everything
// a recipe creates is an ordinary curve and ordinary tools (EQ, FILTER, Echo, Roll) with their
// own envelopes, so the user can correct it afterwards like anything they drew by hand.
//
// Pure: no Svelte, no Tauri, no DOM, and no value imports from the app (type imports only), so
// it runs under plain node (scripts/test-mixedit.mjs). Creating a tool needs `newTool` and the
// overlap rule from state/mixedit.ts; the caller passes them in as a `Kit`.
//
// What the device's mix format can express limits the recipes: a crossfade curve (outgoing is
// muted once the transition ends, the incoming clip is held at its first value before it) and
// the tools of `TOOL_TYPES` with the parameter ranges listed there. The tool semantics used here
// are the ones in notes/mix-audio.md: EQ Lo 0 = flat, -100 = kill; FILTER CutOff negative =
// low-pass, positive = high-pass, |v| < 0.5 = off; Echo/Roll `Wet/Dry` in percent; Roll/Echo
// `Time ` = 2^d beats. They were measured against our own mixer, not the Editor, so they stay
// hypotheses until a mix is heard on the device.
import type { Clip, CurveNode, EnvNode, LoopDoc, MixDoc, ProcessDoc, ToolDoc, ToolSuggestion, TransitionSuggestion } from "../api/mixes";

export type RecipeId = "bassSwap" | "longBlend" | "echoOut" | "filterSweep" | "dropSwap" | "hardCut" | "loopOut" | "loopCut";
/** Where a hard cut falls inside its window. */
export type CutAt = "start" | "end";
/** Where an automatic crossfade sits: "end" = it ends with the outgoing clip, "start" = at the
 * incoming clip's first beat (the backend's own placement). See state/crossfadeLength.ts. */
export type CrossfadePlace = "end" | "start";
export const CROSSFADE_PLACES: CrossfadePlace[] = ["end", "start"];
/** What to do when a tool the recipe wants to add would overlap one the user already has. */
export type ConflictPolicy = "ask" | "skip" | "replace";
/** Which end of an auto transition may move onto the nearest section boundary (detected sections
 * of the two clips, state/sectionTransition.ts). "off" keeps the beat-matched geometry. */
export type SectionEdge = "off" | "start" | "end" | "both";
export const SECTION_EDGES: SectionEdge[] = ["off", "start", "end", "both"];

export type RecipeDef = {
  id: RecipeId;
  label: string;
  /** One sentence on what the transition sounds like. */
  summary: string;
  /** What it creates, one line each, in words (shown before and after building it). */
  makes: string[];
  /** The lengths offered, in bars (a beat is 0.25). */
  lengths: number[];
  /** The length used when the user has not chosen one. */
  bars: number;
};

/** In the order they are offered. */
export const RECIPES: RecipeDef[] = [
  {
    id: "bassSwap",
    label: "Bass swap",
    summary: "Linear crossfade; the incoming bass stays out until the middle, then the two basses trade places on one beat.",
    makes: ["Linear crossfade", "EQ on the outgoing clip: bass cut at the middle", "EQ on the incoming clip: bass in at the middle"],
    lengths: [1, 2, 4, 8, 16, 32],
    bars: 16,
  },
  {
    id: "longBlend",
    label: "Long blend",
    summary: "A slow S-shaped crossfade with the two basses traded gradually through the middle, so the tracks run together for a long time.",
    makes: ["S-shaped crossfade", "EQ on the outgoing clip: bass eased out between 45 % and 65 %", "EQ on the incoming clip: bass eased in between 35 % and 55 %"],
    lengths: [8, 16, 32, 64],
    bars: 32,
  },
  {
    id: "echoOut",
    label: "Echo out",
    summary: "The outgoing track dissolves into its own echoes while the incoming track comes in over the second half.",
    makes: ["Crossfade held until the middle", "Echo on the outgoing clip: wet 0 to 100 % by the middle", "EQ on the incoming clip: bass in at the middle"],
    lengths: [1, 2, 4, 8],
    bars: 4,
  },
  {
    id: "filterSweep",
    label: "Filter sweep",
    summary: "The outgoing track is thinned out with a rising high-pass while the incoming track opens up from a low-pass.",
    makes: ["Linear crossfade", "Filter on the outgoing clip: high-pass rising from 25 % to the end", "Filter on the incoming clip: low-pass opening until 75 %"],
    lengths: [2, 4, 8, 16, 32],
    bars: 8,
  },
  {
    id: "dropSwap",
    label: "Drop swap",
    summary: "The outgoing track builds (bass out, rising high-pass, a beat-repeat on the last bar) and the incoming track hits at full on the last beat. No tape stop or spin-back.",
    makes: ["Crossfade held, then a hard step at the end", "EQ on the outgoing clip: bass cut at the middle", "Filter on the outgoing clip: high-pass rising from the middle", "Roll on the outgoing clip over the last bar"],
    lengths: [2, 4, 8, 16],
    bars: 8,
  },
  {
    id: "hardCut",
    label: "Hard cut",
    summary: "A straight cut to the incoming track with no blend. The incoming clip is held back for the whole length; the cut falls at the end of it (a phrase is 8 bars), or at the start.",
    makes: ["Crossfade with a ten-millisecond step and nothing else"],
    lengths: [0.25, 0.5, 1, 2, 4, 8],
    bars: 8,
  },
  {
    id: "loopOut",
    label: "Loop out",
    summary: "The outgoing track loops a short slice on the beat while the incoming track comes in over it; the basses trade places in the middle.",
    makes: ["Linear crossfade", "Loop on the outgoing clip, from its start or middle of the overlap to the end", "EQ on the outgoing clip: bass cut at the middle", "EQ on the incoming clip: bass in at the middle"],
    lengths: [2, 4, 8, 16],
    bars: 8,
  },
  {
    id: "loopCut",
    label: "Loop cut",
    summary: "The outgoing track loops a bar, then half of it, then half again, down to a quarter of a beat, and the incoming track drops in at the end.",
    makes: ["Crossfade held, then a hard step at the end", "Loops on the outgoing clip, one after another, each half as long as the one before", "Optional: rising high-pass on the outgoing clip"],
    lengths: [2, 4, 8, 16],
    // the default run (1 bar down to a quarter beat, twice each) is 15.5 beats: 4 bars cannot hold it
    bars: 8,
  },
];

export const RECIPE_IDS: RecipeId[] = RECIPES.map((r) => r.id);

export function recipeDef(id: RecipeId): RecipeDef {
  const r = RECIPES.find((x) => x.id === id);
  if (!r) throw new Error(`unknown transition recipe ${id}`);
  return r;
}

/** "4 bars", "1 bar", "2 beats", "1 beat". */
export function lengthLabel(bars: number): string {
  if (bars < 1) {
    const beats = Math.round(bars * 4);
    return `${beats} beat${beats === 1 ? "" : "s"}`;
  }
  // a length worked out from samples is rarely exact: 14.98 bars reads as 15 bars
  const r = Math.round(bars * 10) / 10;
  return `${Number.isInteger(r) ? r : r.toFixed(1)} bar${r === 1 ? "" : "s"}`;
}

/** The offered length of `recipe` nearest to `bars` (by ratio). */
export function nearestLength(recipe: RecipeId, bars: number): number {
  const ls = recipeDef(recipe).lengths;
  if (!(bars > 0) || !Number.isFinite(bars)) return recipeDef(recipe).bars;
  return ls.reduce((best, l) => (Math.abs(Math.log(l / bars)) < Math.abs(Math.log(best / bars)) ? l : best), ls[0]);
}

// ---------------------------------------------------------------------------------------------
// Settings
// ---------------------------------------------------------------------------------------------

/** What a crossfade and its tools do when a clip they sit on moves, is trimmed or retimed:
 * "follow" re-fits them to the new overlap (state/transitionFollow.ts), "stay" leaves them put. */
export type OnMove = "follow" | "stay";
export const ON_MOVES: OnMove[] = ["follow", "stay"];

export type RecipePrefs = {
  /** The recipe the Auto buttons use. */
  recipe: RecipeId;
  /** "Auto crossfade length": the length they use, in bars (default 16); null = each recipe's own
   * natural length. A recipe that does not offer this length uses the nearest one it does, and it
   * is cut to the audio both clips share. */
  bars: number | null;
  /** Where the crossfade sits: "end" = it ends with the outgoing clip (state/crossfadeLength.ts),
   * "start" = at the incoming clip's first beat (the backend's own placement). */
  placeAt: CrossfadePlace;
  /** Where a hard cut falls. */
  cutAt: CutAt;
  /** What to do about tools of the same kind already in the way. */
  onConflict: ConflictPolicy;
  /** Start and/or end the transition on the nearest section boundary of the clips (only where
   * they have detected sections). */
  sectionEdge: SectionEdge;
  /** How far a boundary may be from where the transition would otherwise start or end, in bars. */
  sectionReachBars: number;
  /** What a crossfade and the tools a transition placed do when a clip moves. */
  onMove: OnMove;
  /** Loop out: how long the looped slice is, in beats. */
  loopOutBeats: number;
  /** Loop out: where the loop begins inside the overlap. */
  loopOutStart: LoopOutStart;
  /** Loop cut: the first (longest) loop, in beats; each next one is half as long. */
  loopCutBeats: number;
  /** Loop cut: the shortest loop, in beats. */
  loopCutShortest: number;
  /** Loop cut: how many times each length plays. */
  loopCutRepeats: number;
  /** Loop cut: a high-pass rises over the loops. */
  loopCutRising: boolean;
};

/** Where the loop of a loop out begins: with the overlap, or halfway into it. */
export type LoopOutStart = "start" | "middle";
export const LOOP_OUT_STARTS: LoopOutStart[] = ["start", "middle"];
/** The loop lengths offered, in beats (4 beats = 1 bar). */
export const LOOP_BEATS = [1, 2, 4, 8, 16, 32];
/** The shortest loop of a loop cut that can be asked for, in beats (a thirty-second note is 0.125). */
export const LOOP_SHORTEST = [0.125, 0.25, 0.5, 1];
export const LOOP_REPEATS = { min: 1, max: 4 } as const;
/** "1/4 beat", "1 beat", "2 beats", "1 bar", "4 bars". */
export function loopLengthLabel(beats: number): string {
  if (beats < 1) return `1/${Math.round(1 / beats)} beat`;
  if (beats < 4) return `${beats} beat${beats === 1 ? "" : "s"}`;
  const bars = beats / 4;
  return `${bars} bar${bars === 1 ? "" : "s"}`;
}

/** The setting's words: "starts on a section boundary" (empty when off). */
export function sectionEdgeWords(edge: SectionEdge): string {
  return edge === "start" ? "starts on a section boundary" : edge === "end" ? "ends on a section boundary" : edge === "both" ? "starts and ends on section boundaries" : "";
}

/** The range of `sectionReachBars`. */
export const SECTION_REACH = { min: 1, max: 32 } as const;

export const DEFAULT_RECIPE_PREFS: RecipePrefs = {
  recipe: "bassSwap",
  bars: 16,
  placeAt: "end",
  cutAt: "end",
  onConflict: "ask",
  sectionEdge: "off",
  sectionReachBars: 8,
  onMove: "follow",
  loopOutBeats: 8,
  loopOutStart: "start",
  loopCutBeats: 4,
  loopCutShortest: 0.25,
  loopCutRepeats: 2,
  loopCutRising: false,
};

/** Saved prefs merged over the defaults, ignoring anything unknown or malformed. */
export function mergeRecipePrefs(saved: unknown): RecipePrefs {
  const out: RecipePrefs = { ...DEFAULT_RECIPE_PREFS };
  if (!saved || typeof saved !== "object") return out;
  const s = saved as Record<string, unknown>;
  if (typeof s.recipe === "string" && (RECIPE_IDS as string[]).includes(s.recipe)) out.recipe = s.recipe as RecipeId;
  if (typeof s.bars === "number" && Number.isFinite(s.bars) && s.bars >= 0.25 && s.bars <= 128) out.bars = s.bars;
  // a saved null is the user's choice of "each recipe's own", not a missing value
  else if (s.bars === null) out.bars = null;
  if (typeof s.placeAt === "string" && (CROSSFADE_PLACES as string[]).includes(s.placeAt)) out.placeAt = s.placeAt as CrossfadePlace;
  if (s.cutAt === "start" || s.cutAt === "end") out.cutAt = s.cutAt;
  if (s.onConflict === "ask" || s.onConflict === "skip" || s.onConflict === "replace") out.onConflict = s.onConflict;
  if (typeof s.sectionEdge === "string" && (SECTION_EDGES as string[]).includes(s.sectionEdge)) out.sectionEdge = s.sectionEdge as SectionEdge;
  if (typeof s.sectionReachBars === "number" && Number.isFinite(s.sectionReachBars) && s.sectionReachBars >= SECTION_REACH.min && s.sectionReachBars <= SECTION_REACH.max) {
    out.sectionReachBars = Math.round(s.sectionReachBars);
  }
  if (typeof s.onMove === "string" && (ON_MOVES as string[]).includes(s.onMove)) out.onMove = s.onMove as OnMove;
  if (typeof s.loopOutBeats === "number" && LOOP_BEATS.includes(s.loopOutBeats)) out.loopOutBeats = s.loopOutBeats;
  if (typeof s.loopOutStart === "string" && (LOOP_OUT_STARTS as string[]).includes(s.loopOutStart)) out.loopOutStart = s.loopOutStart as LoopOutStart;
  if (typeof s.loopCutBeats === "number" && LOOP_BEATS.includes(s.loopCutBeats)) out.loopCutBeats = s.loopCutBeats;
  if (typeof s.loopCutShortest === "number" && LOOP_SHORTEST.includes(s.loopCutShortest)) out.loopCutShortest = s.loopCutShortest;
  if (typeof s.loopCutRepeats === "number" && Number.isInteger(s.loopCutRepeats) && s.loopCutRepeats >= LOOP_REPEATS.min && s.loopCutRepeats <= LOOP_REPEATS.max) out.loopCutRepeats = s.loopCutRepeats;
  if (typeof s.loopCutRising === "boolean") out.loopCutRising = s.loopCutRising;
  return out;
}

/** Version stamped on saved prefs. Saves from before it (no stamp) wrote the old default
 * "each recipe's own length" (`bars: null`) whether it was chosen or not, so that value is read as
 * "never chosen" and the current default (16 bars) applies; a stamped null is a real choice. */
export const RECIPE_PREFS_VERSION = 2;

/** What gets saved: the prefs and the version stamp. */
export const stampRecipePrefs = (p: RecipePrefs): RecipePrefs & { v: number } => ({ ...p, v: RECIPE_PREFS_VERSION });

/** Reads saved prefs (`mergeRecipePrefs`), dropping a `bars: null` that carries no version stamp. */
export function loadRecipePrefs(saved: unknown): RecipePrefs {
  if (saved && typeof saved === "object" && (saved as Record<string, unknown>).v !== RECIPE_PREFS_VERSION && (saved as Record<string, unknown>).bars === null) {
    const { bars: _old, ...rest } = saved as Record<string, unknown>;
    return mergeRecipePrefs(rest);
  }
  return mergeRecipePrefs(saved);
}

/** The length to ask the backend for when building `recipe` with `prefs`. */
export function requestedBars(recipe: RecipeId, prefs: Pick<RecipePrefs, "bars">): number {
  return prefs.bars == null ? recipeDef(recipe).bars : nearestLength(recipe, prefs.bars);
}

// ---------------------------------------------------------------------------------------------
// Building a recipe
// ---------------------------------------------------------------------------------------------

/** What building needs from state/mixedit.ts (passed in because this file imports no values). */
export type Kit = {
  newTool: (type: string, start: number, length: number) => ToolDoc;
  /** Ids of the clip's tools of the same kind that overlap `tool`. */
  conflicts: (clip: Clip, tool: ToolDoc) => string[];
  /** A loop over clip-local `[start, start + length)` repeating a slice of `slice` samples `reps`
   * times (`length` defaults to `slice * reps`; a shorter one cuts the last repeat). */
  newLoop: (start: number, slice: number, reps: number, length?: number) => LoopDoc;
  /** Ids of the clip's loops, reverses and ignores that overlap `p` (a loop). */
  regionConflicts: (clip: Clip, p: ProcessDoc) => string[];
  /** Every beat of the clip's grid on the timeline, ascending (empty without a grid). */
  beatsOf: (clip: Clip) => number[];
};

export type LoopSettings = Pick<RecipePrefs, "loopOutBeats" | "loopOutStart" | "loopCutBeats" | "loopCutShortest" | "loopCutRepeats" | "loopCutRising">;

export const DEFAULT_LOOP_SETTINGS: LoopSettings = {
  loopOutBeats: DEFAULT_RECIPE_PREFS.loopOutBeats,
  loopOutStart: DEFAULT_RECIPE_PREFS.loopOutStart,
  loopCutBeats: DEFAULT_RECIPE_PREFS.loopCutBeats,
  loopCutShortest: DEFAULT_RECIPE_PREFS.loopCutShortest,
  loopCutRepeats: DEFAULT_RECIPE_PREFS.loopCutRepeats,
  loopCutRising: DEFAULT_RECIPE_PREFS.loopCutRising,
};

export type RecipeOptions = {
  cutAt: CutAt;
  /** The loop recipes' settings (the defaults when left out). */
  loops?: Partial<LoopSettings>;
  /** The window was moved off the backend's (onto section boundaries): its stock bass-swap tools
   * no longer fit it, so the bass swap is built here like the other recipes. */
  moved?: boolean;
};

/** The step of a hard cut, in samples (about ten milliseconds at 44.1 kHz), so it does not click. */
export const CUT_RAMP = 441;
/** The EQ Lo value that removes the bass (see the header). */
const LO_KILL = -100;

const clamp = (x: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, x));
const round2 = (x: number) => Math.round(x * 100) / 100;

/** A tool envelope over `length` samples from `[pos, value]` points: integer positions, strictly
 * increasing (a later point at the same position wins), starting at 0 and ending at
 * `length - 1` (the last node of a tool's envelope), like a flat envelope from `newTool`. */
export function envelope(length: number, pts: [number, number][]): EnvNode[] {
  const last = Math.max(1, Math.round(length) - 1);
  const sorted = pts
    .map(([p, v]) => ({ pos: clamp(Math.round(p), 0, last), value: round2(v) }))
    .sort((a, b) => a.pos - b.pos);
  if (sorted.length === 0) return [];
  if (sorted[0].pos > 0) sorted.unshift({ pos: 0, value: sorted[0].value });
  if (sorted[sorted.length - 1].pos < last) sorted.push({ pos: last, value: sorted[sorted.length - 1].value });
  const out: EnvNode[] = [];
  for (const n of sorted) {
    if (out.length && out[out.length - 1].pos === n.pos) out[out.length - 1] = n;
    else out.push(n);
  }
  return out;
}

/** A crossfade curve from `[x, value]` points: x and value clamped to 0..1, x strictly
 * increasing, from 0 to 1. */
export function curveOf(pts: [number, number][]): CurveNode[] {
  const sorted = pts.map(([x, v]) => ({ x: clamp(x, 0, 1), value: clamp(v, 0, 1) })).sort((a, b) => a.x - b.x);
  if (sorted[0].x > 0) sorted.unshift({ x: 0, value: sorted[0].value });
  if (sorted[sorted.length - 1].x < 1) sorted.push({ x: 1, value: sorted[sorted.length - 1].value });
  const out: CurveNode[] = [];
  for (const n of sorted) {
    if (out.length && Math.abs(out[out.length - 1].x - n.x) < 1e-9) out[out.length - 1] = n;
    else out.push({ x: Math.round(n.x * 1e6) / 1e6, value: round2(n.value) });
  }
  return out;
}

/** A hard step of the crossfader: 0 until the step, 1 after, over `ramp` samples of `length`. */
function stepCurve(length: number, at: CutAt): CurveNode[] {
  const r = Math.min(0.5, CUT_RAMP / Math.max(1, length));
  return at === "start" ? curveOf([[0, 0], [r, 1], [1, 1]]) : curveOf([[0, 0], [1 - r, 0], [1, 1]]);
}

// ---------------------------------------------------------------------------------------------
// Loops on the beat grid
// ---------------------------------------------------------------------------------------------

/** A clip's beats as a number line: `pos(time)` is the beat index with the fraction of the way to
 * the next beat, `time(pos)` the way back. Uses the real beat times, so a grid that drifts still
 * gives loops that start and end on its beats; past either end it carries on at the end beat's
 * length. Needs at least two beats. */
export type BeatMap = { pos: (time: number) => number; time: (pos: number) => number };

export function beatMap(beats: number[]): BeatMap {
  const n = beats.length;
  const seg = (i: number) => clamp(i, 0, n - 2);
  return {
    time: (p) => {
      const i = seg(Math.floor(p));
      return beats[i] + (p - i) * (beats[i + 1] - beats[i]);
    },
    pos: (t) => {
      let lo = 0;
      let hi = n - 2;
      while (lo < hi) {
        const mid = (lo + hi + 1) >> 1;
        if (beats[mid] <= t) lo = mid;
        else hi = mid - 1;
      }
      return lo + (t - beats[lo]) / (beats[lo + 1] - beats[lo]);
    },
  };
}

/** One loop of a plan, on the timeline: from `start`, `reps` repeats of `slice` samples. */
export type LoopStage = { start: number; slice: number; reps: number };

/** Loop out: one loop from a beat of the outgoing clip to the end `end` of the window. The slice
 * is `beats` beats long (halved until two of them fit), the last repeat is cut at the end of the
 * window, where the crossfade has silenced the outgoing clip anyway. `null` when the window holds
 * less than two beats from the loop's start. */
export function planLoopOut(map: BeatMap, start: number, end: number, beats: number, at: LoopOutStart): (LoopStage & { length: number; beats: number }) | null {
  for (const from of at === "middle" ? [(start + end) / 2, start] : [start]) {
    const first = Math.ceil(map.pos(from) - 1e-6);
    const room = map.pos(end) - first;
    if (room < 2) continue;
    let nb = beats;
    while (nb > 1 && nb * 2 > room + 1e-6) nb /= 2;
    const t0 = Math.round(map.time(first));
    const slice = Math.round(map.time(first + nb)) - t0;
    const length = Math.round(end) - t0;
    if (slice < 1 || length < 2 * slice - 1) continue;
    return { start: t0, slice, reps: Math.ceil(length / slice), length, beats: nb };
  }
  return null;
}

/** Loop cut: loops one after another on the beat grid, from `beats` beats down to `shortest`, each
 * half as long as the one before and played `repeats` times, so the last one ends where the window
 * does (within the shortest loop, which the shortest loop is repeated to fill). The first starts on
 * a beat. When they do not all fit between `start` and `end` the longest are left out. Each
 * stage starts where the one before ended, to the sample. `null` when not even one fits. */
export function planLoopCut(map: BeatMap, start: number, end: number, beats: number, shortest: number, repeats: number): { stages: LoopStage[]; lengths: number[]; left: number } | null {
  let lengths: number[] = [];
  for (let l = beats; l >= shortest - 1e-9; l /= 2) lengths.push(l);
  if (lengths.length === 0) lengths = [shortest];
  const first = Math.ceil(map.pos(start) - 1e-6);
  const room = map.pos(end) - first;
  const total = (ls: number[]) => ls.reduce((a, b) => a + b, 0) * repeats;
  const all = lengths.length;
  while (lengths.length > 0 && total(lengths) > room + 1e-6) lengths = lengths.slice(1);
  if (lengths.length === 0) return null;
  const run = total(lengths);
  const begin = Math.max(first, Math.floor(map.pos(end) - run + 1e-6));
  // the beats left over before the end go to the last (shortest) loop as extra repeats
  const extra = Math.max(0, Math.floor((map.pos(end) - begin - run) / lengths[lengths.length - 1] + 1e-6));
  const stages: LoopStage[] = [];
  let pos = begin;
  let at = Math.round(map.time(begin));
  lengths.forEach((l, i) => {
    const reps = repeats + (i === lengths.length - 1 ? extra : 0);
    // rounded down: the run then never reaches past the window it was fitted to
    const slice = Math.max(1, Math.floor(map.time(pos + l) - map.time(pos) + 1e-6));
    stages.push({ start: at, slice, reps });
    at += slice * reps;
    pos += l * reps;
  });
  return { stages, lengths, left: all - lengths.length };
}

/** Builds `recipe` on the geometry of `base` (a `mixAutoTransition` suggestion for the length
 * wanted). Returns a suggestion in the same shape, ready for `applyTransition`. Throws a sentence
 * for the user when the clips share less audio than the recipe can use. `doc` is the document
 * the suggestion was made for (its clips decide which tools are in the way). */
export function buildRecipe(doc: MixDoc, base: TransitionSuggestion, recipe: RecipeId, opts: RecipeOptions, kit: Kit): TransitionSuggestion {
  const def = recipeDef(recipe);
  // the bass swap is the backend's own suggestion and works on any overlap (under a bar it has no tools)
  if (recipe === "bassSwap" && base.transition.between && !opts.moved) return { ...base, transition: { ...base.transition, curve: curveOf([[0, 0], [1, 1]]) } };
  if (base.bars < def.lengths[0] && !(recipe === "bassSwap" && opts.moved)) {
    throw new Error(
      `Only ${lengthLabel(base.bars)} of the two clips overlap here; a ${def.label.toLowerCase()} needs at least ${lengthLabel(def.lengths[0])}. Move the later clip earlier, or pick a shorter recipe.`,
    );
  }
  const t = base.transition;
  if (!t.between) throw new Error("the suggestion does not say which clips it joins");

  const { from, to } = t.between;
  const L = t.length;
  const B = base.beat;
  const last = Math.max(1, Math.round(L) - 1);
  /** A position `frac` of the way through the window, snapped to a beat. */
  const at = (frac: number) => clamp(Math.round((frac * L) / B) * B, 0, last);

  const tools: ToolSuggestion[] = [];
  const add = (clipId: string, type: string, offset: number, length: number, params: Record<string, EnvNode[]>) => {
    const clip = doc.clips.find((c) => c.id === clipId);
    if (!clip) throw new Error(`no clip ${clipId}`);
    const tool = kit.newTool(type, t.start - clip.start + offset, length);
    tool.params = { ...tool.params, ...params };
    tools.push({ clipId, tool, conflicts: kit.conflicts(clip, tool) });
  };
  /** EQ with only Lo moving. */
  const eqLo = (clipId: string, pts: [number, number][]) => add(clipId, "EQ", 0, L, { "EQ-Lo": envelope(L, pts) });

  const lp: LoopSettings = { ...DEFAULT_LOOP_SETTINGS, ...opts.loops };
  const fromClip = doc.clips.find((c) => c.id === from);
  if (!fromClip) throw new Error(`no clip ${from}`);
  const addLoop = (st: LoopStage, length = st.slice * st.reps) => {
    const loop = kit.newLoop(st.start - fromClip.start, st.slice, st.reps, length);
    tools.push({ clipId: from, tool: loop, conflicts: kit.regionConflicts(fromClip, loop) });
  };
  /** The outgoing clip's beats as a number line; without a grid, even beats of the transition's own length. */
  const beatLine = (): BeatMap => {
    const own = kit.beatsOf(fromClip).filter((b) => Number.isFinite(b));
    if (own.length >= 2 && own[own.length - 1] > t.start + L) return beatMap(own);
    notes.push("no beat grid on the outgoing clip, loops placed by tempo");
    return beatMap(Array.from({ length: Math.ceil(L / B) + 8 }, (_, i) => t.start + (i - 4) * B));
  };

  let curve: CurveNode[];
  // the backend's "under a bar" note is about the bass swap's tools, which no other recipe makes
  const notes = base.notes.filter((n) => !n.includes("no bass swap tools"));

  switch (recipe) {
    case "bassSwap": {
      // the backend's own bass swap (timing.rs `eq_swap_tool`) on a moved window: Lo held, then
      // killed over the beat before the middle (outgoing) or opened over the beat after it
      // (incoming); under a bar there are no tools
      curve = curveOf([[0, 0], [1, 1]]);
      if (base.bars >= 1) {
        const swap = at(0.5);
        eqLo(from, [[0, 0], [Math.max(0, swap - B), 0], [swap, LO_KILL], [last, LO_KILL]]);
        eqLo(to, [[0, LO_KILL], [swap, LO_KILL], [Math.min(last, swap + B), 0], [last, 0]]);
      } else {
        notes.push(`${Math.round(base.bars * 4)} beat(s) is under a bar: no bass swap tools`);
      }
      break;
    }
    case "longBlend": {
      curve = curveOf([[0, 0], [0.25, 0.15], [0.75, 0.85], [1, 1]]);
      eqLo(from, [[0, 0], [at(0.45), 0], [at(0.65), LO_KILL], [last, LO_KILL]]);
      eqLo(to, [[0, LO_KILL], [at(0.35), LO_KILL], [at(0.55), 0], [last, 0]]);
      break;
    }
    case "echoOut": {
      curve = curveOf([[0, 0], [0.5, 0], [1, 1]]);
      add(from, "Echo", 0, L, { "Echo-Wet/Dry": envelope(L, [[0, 0], [at(0.25), 0], [at(0.5), 100], [last, 100]]) });
      eqLo(to, [[0, LO_KILL], [at(0.5), LO_KILL], [at(0.5) + B, 0], [last, 0]]);
      break;
    }
    case "filterSweep": {
      curve = curveOf([[0, 0], [1, 1]]);
      add(from, "FILTER", 0, L, { "FILTER-CutOff": envelope(L, [[0, 0], [at(0.25), 0], [last, 100]]) });
      add(to, "FILTER", 0, L, { "FILTER-CutOff": envelope(L, [[0, -100], [at(0.75), 0], [last, 0]]) });
      break;
    }
    case "dropSwap": {
      curve = stepCurve(L, "end");
      const mid = at(0.5);
      eqLo(from, [[0, 0], [mid, 0], [mid + B, LO_KILL], [last, LO_KILL]]);
      add(from, "FILTER", mid, L - mid, { "FILTER-CutOff": envelope(L - mid, [[0, 0], [L - mid, 80]]) });
      // the beat-repeat covers the last bar (or half the window when that is shorter), in whole beats
      const roll = clamp(Math.round(Math.min(4 * B, L / 2) / B) * B, B, L);
      const half = Math.round(roll / B / 2) * B;
      add(from, "Roll", L - roll, roll, {
        // an eighth for the first half, a sixteenth for the second: the repeat tightens
        "Roll-Time ": envelope(roll, [[0, -1], [Math.max(1, half - 1), -1], [Math.max(2, half), -2], [roll, -2]]),
        "Roll-Wet/Dry": envelope(roll, [[0, 40], [roll, 90]]),
      });
      break;
    }
    case "hardCut": {
      curve = stepCurve(L, opts.cutAt);
      break;
    }
    case "loopOut": {
      curve = curveOf([[0, 0], [1, 1]]);
      const plan = planLoopOut(beatLine(), t.start, t.start + L, lp.loopOutBeats, lp.loopOutStart);
      if (!plan) throw new Error("There is no room to loop here: the overlap holds fewer than two beats.");
      if (plan.beats !== lp.loopOutBeats) notes.push(`loop shortened to ${loopLengthLabel(plan.beats)} to fit`);
      addLoop(plan, plan.length);
      const swap = at(0.5);
      eqLo(from, [[0, 0], [Math.max(0, swap - B), 0], [swap, LO_KILL], [last, LO_KILL]]);
      eqLo(to, [[0, LO_KILL], [swap, LO_KILL], [Math.min(last, swap + B), 0], [last, 0]]);
      break;
    }
    case "loopCut": {
      curve = stepCurve(L, "end");
      const plan = planLoopCut(beatLine(), t.start, t.start + L, lp.loopCutBeats, lp.loopCutShortest, lp.loopCutRepeats);
      if (!plan) throw new Error("There is no room for a loop cut here: the overlap is shorter than one loop.");
      if (plan.left > 0) notes.push(`longest loop${plan.left === 1 ? "" : "s"} left out to fit`);
      for (const st of plan.stages) addLoop(st);
      if (lp.loopCutRising) {
        const first = plan.stages[0].start;
        const end = t.start + L;
        add(from, "FILTER", first - t.start, end - first, { "FILTER-CutOff": envelope(end - first, [[0, 0], [end - first, 80]]) });
      }
      break;
    }
    default:
      throw new Error(`no recipe ${recipe}`);
  }

  return { ...base, transition: { ...t, curve }, tools, notes };
}

/** The loops of `c` that make up a loop recipe on the window `w`: the last one ends at the window's
 * end (within `tol` samples) and each earlier one ends where the next begins, to the sample. Empty
 * when none does. */
export function loopRun(c: Clip, w: { start: number; end: number }, tol: number): LoopDoc[] {
  const endOf = (p: LoopDoc) => c.start + p.start + p.length;
  const all = c.processes.filter((p): p is LoopDoc => p.kind === "loop" && !p.bypassed && c.start + p.start >= w.start - tol && endOf(p) <= w.end + tol);
  let cur: LoopDoc | undefined = all.filter((p) => Math.abs(endOf(p) - w.end) <= tol).sort((a, b) => endOf(b) - endOf(a))[0];
  const run: LoopDoc[] = [];
  while (cur && !run.includes(cur)) {
    run.unshift(cur);
    const first: LoopDoc = cur;
    cur = all.find((p) => p !== first && Math.abs(endOf(p) - (c.start + first.start)) <= 2);
  }
  return run;
}

export type InTheWay = {
  id: string;
  clipId: string;
  /** Effect name of the tool ("EQ", "FILTER", ...), or "loop" / "reverse" / "ignore". */
  toolType: string;
  /** "overlaps": a tool of the same kind as one the recipe adds, in the same place. "inside": a
   * tool of the transition being rebuilt, lying within its window. */
  why: "overlaps" | "inside";
};

/** The user's tools that building `s` would touch: those that overlap a tool it adds (the Editor
 * allows one tool of a kind in a place), and, when it rebuilds the transition `replacing`, every
 * tool of its two clips that lies within that transition's window (the previous recipe's tools).
 * Nothing is removed from this list without the user's say (settings: `onConflict`). */
export function toolsInTheWay(doc: MixDoc, s: TransitionSuggestion, replacing: { start: number; length: number; between: { from: string; to: string } | null } | null = null): InTheWay[] {
  const seen = new Set<string>();
  const out: InTheWay[] = [];
  const push = (clip: Clip, p: Clip["processes"][number], why: InTheWay["why"]) => {
    if (seen.has(p.id)) return;
    seen.add(p.id);
    // a loop, reverse or ignore has no effect to name: its kind is the name
    out.push({ id: p.id, clipId: clip.id, toolType: p.kind === "tool" ? (p.effects[0]?.name ?? p.toolType) : p.kind, why });
  };
  for (const ts of s.tools) {
    const clip = doc.clips.find((c) => c.id === ts.clipId);
    if (!clip) continue;
    for (const id of ts.conflicts) {
      const p = clip.processes.find((x) => x.id === id);
      if (p) push(clip, p, "overlaps");
    }
  }
  if (replacing?.between) {
    const lo = replacing.start - 1;
    const hi = replacing.start + replacing.length + 1;
    for (const id of [replacing.between.from, replacing.between.to]) {
      const clip = doc.clips.find((c) => c.id === id);
      if (!clip) continue;
      for (const p of clip.processes) {
        if (p.kind === "tool" && clip.start + p.start >= lo && clip.start + p.start + p.length <= hi) push(clip, p, "inside");
      }
      // the loops of a loop out or loop cut (only the run that ends with the window: a loop of the
      // user's somewhere in it is theirs)
      if (id === replacing.between.from) {
        for (const p of loopRun(clip, { start: replacing.start, end: replacing.start + replacing.length }, replacing.length / 8)) push(clip, p, "inside");
      }
    }
  }
  return out;
}

/** One line per tool a built recipe adds, in words (`name(clipId)` resolves a clip's title,
 * `label` a tool type's name as the card presents it). */
export function describeTools(s: TransitionSuggestion, name: (clipId: string) => string, label: (type: string) => string = (t) => t): string[] {
  return s.tools.map((t) => `${label(t.tool.kind === "tool" ? t.tool.toolType : t.tool.kind)} on "${name(t.clipId)}"`);
}
