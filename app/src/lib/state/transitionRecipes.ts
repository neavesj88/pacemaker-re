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
import type { Clip, CurveNode, EnvNode, MixDoc, ToolDoc, ToolSuggestion, TransitionSuggestion } from "../api/mixes";

export type RecipeId = "bassSwap" | "longBlend" | "echoOut" | "filterSweep" | "dropSwap" | "hardCut";
/** Where a hard cut falls inside its window. */
export type CutAt = "start" | "end";
/** What to do when a tool the recipe wants to add would overlap one the user already has. */
export type ConflictPolicy = "ask" | "skip" | "replace";

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
  const t = Number.isInteger(bars) ? String(bars) : bars.toFixed(1);
  return `${t} bar${bars === 1 ? "" : "s"}`;
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

export type RecipePrefs = {
  /** The recipe the Auto buttons use. */
  recipe: RecipeId;
  /** The length they use, in bars; null = each recipe's own natural length. A recipe that does
   * not offer this length uses the nearest one it does. */
  bars: number | null;
  /** Where a hard cut falls. */
  cutAt: CutAt;
  /** What to do about tools of the same kind already in the way. */
  onConflict: ConflictPolicy;
};

export const DEFAULT_RECIPE_PREFS: RecipePrefs = { recipe: "bassSwap", bars: null, cutAt: "end", onConflict: "ask" };

/** Saved prefs merged over the defaults, ignoring anything unknown or malformed. */
export function mergeRecipePrefs(saved: unknown): RecipePrefs {
  const out: RecipePrefs = { ...DEFAULT_RECIPE_PREFS };
  if (!saved || typeof saved !== "object") return out;
  const s = saved as Record<string, unknown>;
  if (typeof s.recipe === "string" && (RECIPE_IDS as string[]).includes(s.recipe)) out.recipe = s.recipe as RecipeId;
  if (typeof s.bars === "number" && Number.isFinite(s.bars) && s.bars >= 0.25 && s.bars <= 128) out.bars = s.bars;
  if (s.cutAt === "start" || s.cutAt === "end") out.cutAt = s.cutAt;
  if (s.onConflict === "ask" || s.onConflict === "skip" || s.onConflict === "replace") out.onConflict = s.onConflict;
  return out;
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
};

export type RecipeOptions = { cutAt: CutAt };

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

/** Builds `recipe` on the geometry of `base` (a `mixAutoTransition` suggestion for the length
 * wanted). Returns a suggestion in the same shape, ready for `applyTransition`. Throws a sentence
 * for the user when the clips share less audio than the recipe can use. `doc` is the document
 * the suggestion was made for (its clips decide which tools are in the way). */
export function buildRecipe(doc: MixDoc, base: TransitionSuggestion, recipe: RecipeId, opts: RecipeOptions, kit: Kit): TransitionSuggestion {
  const def = recipeDef(recipe);
  // the bass swap is the backend's own suggestion and works on any overlap (under a bar it has no tools)
  if (recipe === "bassSwap" && base.transition.between) return { ...base, transition: { ...base.transition, curve: curveOf([[0, 0], [1, 1]]) } };
  if (base.bars < def.lengths[0]) {
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

  let curve: CurveNode[];
  const notes = [...base.notes];

  switch (recipe) {
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
    default:
      throw new Error(`no recipe ${recipe}`);
  }

  return { ...base, transition: { ...t, curve }, tools, notes };
}

export type InTheWay = {
  id: string;
  clipId: string;
  /** Effect name of the tool ("EQ", "FILTER", ...). */
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
    if (p.kind !== "tool" || seen.has(p.id)) return;
    seen.add(p.id);
    out.push({ id: p.id, clipId: clip.id, toolType: p.effects[0]?.name ?? p.toolType, why });
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
    }
  }
  return out;
}

/** One line per tool a built recipe adds, in words (`name(clipId)` resolves a clip's title,
 * `label` a tool type's name as the card presents it). */
export function describeTools(s: TransitionSuggestion, name: (clipId: string) => string, label: (type: string) => string = (t) => t): string[] {
  return s.tools.map((t) => `${label(t.tool.kind === "tool" ? t.tool.toolType : t.tool.kind)} on "${name(t.clipId)}"`);
}
