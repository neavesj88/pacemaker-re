// "Create a mix from the set": the part that talks to the backend. The layout itself is
// buildMix.ts (pure); what is done to each hop is planMix in solver.ts, which the dialog shows
// first. Nothing is written until the whole mix has been built in memory and the backend's own
// validation (`mixValidate`, the same check a save makes) accepts it.
import {
  mixAutoTransition,
  mixBeatmatch,
  mixDelete,
  mixMakeClip,
  mixNew,
  mixOpen,
  mixSave,
  mixTrackInfo,
  mixValidate,
  type MixSaved,
  type TrackInfo,
} from "$lib/api/mixes";
import * as E from "$lib/state/mixedit";
import { sections } from "$lib/state/sections.svelte";
import { alignSuggestion } from "$lib/state/sectionTransition";
import { buildRecipe, recipeDef, requestedBars, sectionEdgeWords, type RecipePrefs } from "$lib/state/transitionRecipes";
import { buildSetDoc, type BuildKit, type BuildResult } from "./buildMix";
import { planMix, type MixPlan, type SetPrefs, type SetTrack } from "./solver";

export type Unusable = { id: number; why: string };

export type Preflight = {
  /** The tracks the mix will use, in play order (the set minus the unusable ones). */
  usable: SetTrack[];
  /** Tracks that cannot go in a mix, with the reason the backend gives. */
  unusable: Unusable[];
  /** Library facts per track id (grid or not, etc.). */
  infos: Map<number, TrackInfo>;
};

/** Asks the backend about every track of the set. Writes nothing. */
export async function preflight(root: string, order: SetTrack[], prefs: SetPrefs): Promise<Preflight> {
  const list = await mixTrackInfo(root, order.map((t) => t.id));
  const infos = new Map(list.map((i) => [i.trackId, i]));
  const unusable: Unusable[] = [];
  const usable: SetTrack[] = [];
  for (const t of order) {
    const i = infos.get(t.id);
    if (!i || !i.available) unusable.push({ id: t.id, why: i?.problem ?? "the track is not in the library" });
    else usable.push(t);
  }
  return { usable, unusable, infos };
}

/** The tempo change above which a hop is not beat-matched: its own setting, else the set's largest tempo jump. */
export const blendLimit = (prefs: SetPrefs): number => prefs.blendJumpPct ?? prefs.maxBpmJumpPct;

/** What will be done to each hop with the current settings. The dialog shows it and `createMix` follows it. */
export function planFor(pre: Preflight, prefs: SetPrefs): MixPlan {
  return planMix(pre.usable, (id) => pre.infos.get(id)?.grid != null, { beatmatch: prefs.beatmatch, mixTempo: prefs.mixTempo, halfDouble: prefs.halfDouble, maxJumpPct: blendLimit(prefs) });
}

/** The recipe and length the new mix will use, in words (they are the Mixes view's auto-transition settings). */
export function transitionWords(rp: RecipePrefs): string {
  const bars = requestedBars(rp.recipe, rp);
  const edge = sectionEdgeWords(rp.sectionEdge);
  return `${recipeDef(rp.recipe).label}, ${bars < 1 ? `${Math.round(bars * 4)} beat${Math.round(bars * 4) === 1 ? "" : "s"}` : `${bars} bar${bars === 1 ? "" : "s"}`}${edge ? `, ${edge} where sections are detected` : ""}`;
}

/** The editing functions for a build with these transition settings. The recipe step honours the
 * "start / end on a section boundary" setting the same way the mix editor's Auto buttons do. */
const kitFor = (rp: RecipePrefs): BuildKit => ({
  SR: E.SR,
  addClip: E.addClip,
  setClipBpm: E.setClipBpm,
  resizeClip: (d, id, edge, pos) => E.resizeClip(d, id, edge, pos),
  applyBeatmatch: E.applyBeatmatch,
  applyTransition: E.applyTransition,
  clipOf: E.clipOf,
  effectiveBpm: E.effectiveBpm,
  recipe: (doc, base, recipe, cutAt) => {
    const aligned = alignSuggestion(doc, base, rp, (id) => sections.of(id), recipeDef(recipe).lengths[0] * 4 * base.beat);
    return buildRecipe(doc, aligned.suggestion, recipe, { cutAt, moved: aligned.moved }, E.recipeKit);
  },
});

export type Created = { saved: MixSaved; result: BuildResult };

/**
 * Creates the mix called `name` from a preflight and the plan `planFor` made. The empty mix is made first (the document has to
 * exist to be built on); if anything fails before the save, that empty mix is deleted again and
 * the error is thrown, so a failed attempt leaves nothing behind.
 */
export async function createMix(
  root: string,
  name: string,
  pre: Preflight,
  plan: MixPlan,
  titleOf: (id: number) => string,
  prefs: SetPrefs,
  recipe: RecipePrefs,
  progress?: (done: number, total: number, title: string) => void,
): Promise<Created> {
  const id = await mixNew(root, name);
  try {
    const base = await mixOpen(root, id);
    const result = await buildSetDoc(
      base,
      pre.usable.map((t) => ({ id: t.id, title: titleOf(t.id) })),
      plan,
      { beatmatch: prefs.beatmatch, maxTrackSecs: prefs.maxTrackMins != null ? prefs.maxTrackMins * 60 : null, bars: requestedBars(recipe.recipe, recipe), recipe: recipe.recipe, cutAt: recipe.cutAt, overlapMaxFrac: prefs.overlapCapPct / 100, replaceConflicts: prefs.onToolConflict === "replace" },
      {
        makeClip: (trackId) => mixMakeClip(root, trackId, null),
        beatmatch: (doc, clipId) => mixBeatmatch(root, doc, clipId),
        autoTransition: (doc, from, to, bars) => mixAutoTransition(root, doc, from, to, bars),
      },
      kitFor(recipe),
      progress,
    );
    const problems = await mixValidate(root, result.doc);
    if (problems.length > 0) throw new Error(`the mix would not save: ${problems.join("; ")}`);
    const saved = await mixSave(root, id, result.doc);
    return { saved, result };
  } catch (e) {
    try {
      await mixDelete(root, id);
    } catch (d) {
      throw new Error(`${e instanceof Error ? e.message : String(e)} (the empty mix “${name}” could not be removed either: ${d instanceof Error ? d.message : String(d)})`);
    }
    throw e;
  }
}
