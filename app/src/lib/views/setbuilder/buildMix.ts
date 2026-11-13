// Lays a set out as a mix document: the clips one after another, each beat-matched to the one
// before and joined with the user's auto-transition recipe, using the same backend suggestions
// and the same pure editing functions as the mix editor (state/mixedit.ts, state/transitionRecipes.ts).
//
// Pure apart from the calls in `BuildDeps`, and with no value imports from the app (the editing
// functions come in through `BuildKit`), so scripts/test-mixedit.mjs runs it under plain node
// with a fake backend. `createMix.ts` wires the real commands and the real editing functions.
import type { BeatmatchSuggestion, Clip, MixDoc, TransitionSuggestion } from "../../api/mixes";
import type { CutAt, RecipeId } from "../../state/transitionRecipes";
import type { MixPlan } from "./solver";

/** The backend calls the builder makes. */
export type BuildDeps = {
  /** A new clip for a library track (channel 0 at 0, tempo untouched). */
  makeClip: (trackId: number) => Promise<Clip>;
  beatmatch: (doc: MixDoc, clipId: string) => Promise<BeatmatchSuggestion>;
  autoTransition: (doc: MixDoc, fromClipId: string, toClipId: string, bars: number) => Promise<TransitionSuggestion>;
};

/** The editing functions the builder uses (state/mixedit.ts and state/transitionRecipes.ts). */
export type BuildKit = {
  SR: number;
  addClip: (doc: MixDoc, clip: Clip, start: number, channel?: number) => MixDoc;
  setClipBpm: (doc: MixDoc, clipId: string, bpm: number) => MixDoc;
  resizeClip: (doc: MixDoc, id: string, edge: "left" | "right", pos: number) => MixDoc;
  applyBeatmatch: (doc: MixDoc, s: BeatmatchSuggestion) => MixDoc;
  applyTransition: (doc: MixDoc, s: TransitionSuggestion, replaceConflicts: boolean) => MixDoc;
  clipOf: (doc: MixDoc, id: string) => Clip | undefined;
  effectiveBpm: (c: Clip, local: number) => number;
  /** The recipe applied to a backend suggestion (buildRecipe with the editor's kit). */
  recipe: (doc: MixDoc, base: TransitionSuggestion, recipe: RecipeId, cutAt: CutAt) => TransitionSuggestion;
};

export type BuildOptions = {
  beatmatch: boolean;
  /** Longest a track plays in the mix, seconds; null = whole tracks. */
  maxTrackSecs: number | null;
  /** Transition length in bars (already resolved from the user's settings). */
  bars: number;
  recipe: RecipeId;
  cutAt: CutAt;
  /** The most a blend may overlap, as a fraction of the shorter track (default 0.4). */
  overlapMaxFrac?: number;
  /** A transition tool that lands on one already there replaces it instead of being left out. */
  replaceConflicts?: boolean;
};

export type BuildTrack = { id: number; title: string };

export type BuildResult = {
  doc: MixDoc;
  /** Clip ids in play order. */
  clipIds: string[];
  /** Things the user should know, each a sentence. */
  notes: string[];
  /** Tools of a recipe left out because one of the clip's own was in the way. */
  toolsSkipped: number;
  /** How far each clip was actually stretched from its track's tempo, percent. */
  stretchPct: number[];
};

const errText = (e: unknown) => (typeof e === "string" ? e : e instanceof Error ? e.message : String(e));

/**
 * Builds the mix content on `base` (an empty mix document). `tracks` is the play order after the
 * unusable ones were removed and `plan` was made for exactly those tracks. A hop that cannot be
 * blended (the backend refuses it) falls back to starting the track after the last one, and says so.
 */
export async function buildSetDoc(
  base: MixDoc,
  tracks: readonly BuildTrack[],
  plan: MixPlan,
  o: BuildOptions,
  deps: BuildDeps,
  kit: BuildKit,
  progress?: (done: number, total: number, title: string) => void,
): Promise<BuildResult> {
  const SR = kit.SR;
  let doc = base;
  const clipIds: string[] = [];
  const notes: string[] = [];
  let toolsSkipped = 0;

  /** Cuts a clip to the longest play time, on a whole number of bars. */
  const trim = (d: MixDoc, id: string): MixDoc => {
    const c = kit.clipOf(d, id);
    if (!c || !o.maxTrackSecs) return d;
    const cap = o.maxTrackSecs * SR;
    if (c.visibleEnd - c.visibleStart <= cap) return d;
    const bar = (60 / (kit.effectiveBpm(c, c.trimStart) || 120)) * 4 * SR;
    const bars = Math.floor(cap / bar);
    return kit.resizeClip(d, id, "right", c.visibleStart + (bars >= 1 ? bars * bar : cap));
  };

  for (let i = 0; i < tracks.length; i++) {
    const t = tracks[i];
    progress?.(i, tracks.length, t.title);
    const fresh = await deps.makeClip(t.id);
    const channel = i % 2;
    const label = `"${t.title}"`;

    /** The tempo a track that starts a run must be set to (the rest of the run follows it). */
    const runTempo = Math.abs(plan.stretchPct[i]) > 1e-9 ? plan.tempos[i] : null;

    if (i === 0) {
      doc = kit.addClip(doc, fresh, 0, channel);
      if (runTempo !== null) doc = kit.setClipBpm(doc, fresh.id, runTempo);
      doc = trim(doc, fresh.id);
      clipIds.push(fresh.id);
      continue;
    }

    const pair = plan.pairs[i - 1];
    const prevId = clipIds[i - 1];
    const prev = kit.clipOf(doc, prevId);
    if (!prev) throw new Error(`internal: the clip before ${label} is missing`);

    /** Starts the track when the last one ends. */
    const sequence = (reason: string, tempo: number | null) => {
      doc = kit.addClip(doc, fresh, prev.visibleEnd, channel);
      if (tempo !== null) doc = kit.setClipBpm(doc, fresh.id, tempo);
      doc = trim(doc, fresh.id);
      clipIds.push(fresh.id);
      notes.push(reason);
    };

    if (pair.mode === "sequence") {
      const why =
        pair.why === "noGrid"
          ? "a track here has no beat grid"
          : pair.why === "noTempo"
            ? "a track here has no tempo"
            : pair.why === "bigJump"
              ? `its tempo is ${Math.abs(pair.jumpPct ?? 0).toFixed(1)} % ${(pair.jumpPct ?? 0) >= 0 ? "above" : "below"} the one before, more than the limit for beat-matching`
              : "it is at half or double the tempo of the one before";
      sequence(`${label} starts when the track before it ends, with no overlap: ${why}.`, runTempo);
      continue;
    }

    // blend: overlap the end of the last track, then align and join
    const bar = (60 / (kit.effectiveBpm(prev, Math.max(0, prev.length - prev.trimEnd - 1)) || 120)) * 4 * SR;
    const prevDur = prev.visibleEnd - prev.visibleStart;
    const thisDur = fresh.length - fresh.trimStart - fresh.trimEnd;
    const frac = o.overlapMaxFrac ?? 0.4;
    const overlap = Math.min(o.bars * bar, frac * prevDur, frac * thisDur);
    const before = doc;
    try {
      doc = kit.addClip(doc, fresh, Math.max(0, prev.visibleEnd - overlap), channel);
      if (o.beatmatch) {
        const s = await deps.beatmatch(doc, fresh.id);
        doc = kit.applyBeatmatch(doc, s);
        for (const n of s.notes) notes.push(`${label}: ${n}.`);
      }
      doc = trim(doc, fresh.id);
      const suggested = await deps.autoTransition(doc, prevId, fresh.id, o.bars);
      const built = kit.recipe(doc, suggested, o.recipe, o.cutAt);
      toolsSkipped += built.tools.filter((x) => x.conflicts.length > 0).length;
      doc = kit.applyTransition(doc, built, o.replaceConflicts === true);
      clipIds.push(fresh.id);
    } catch (e) {
      doc = before;
      sequence(`${label} could not be blended with the track before it (${errText(e)}), so it starts when that track ends.`, null);
    }
  }
  progress?.(tracks.length, tracks.length, "");

  const stretchPct = clipIds.map((id) => {
    const c = kit.clipOf(doc, id);
    return c && c.bpm > 0 ? (kit.effectiveBpm(c, c.trimStart) / c.bpm - 1) * 100 : 0;
  });
  return { doc, clipIds, notes, toolsSkipped, stretchPct };
}
