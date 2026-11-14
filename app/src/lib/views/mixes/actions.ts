// Editor actions that combine the document store, the view state and the user's feedback
// (toasts). Shared by the timeline, the inspector, the context menus and the keyboard.
import type { Clip, MixDoc } from "$lib/api/mixes";
import * as E from "$lib/state/mixedit";
import { mixes } from "$lib/state/mixes.svelte";
import { alignSuggestion } from "$lib/state/sectionTransition";
import { toast } from "$lib/state/toast.svelte";
import { vocal } from "$lib/state/vocal.svelte";
import {
  buildRecipe,
  describeTools,
  lengthLabel,
  recipeDef,
  requestedBars,
  toolsInTheWay,
  type InTheWay,
  type RecipeId,
} from "$lib/state/transitionRecipes";
import { confirm } from "./confirm.svelte";
import { SR, mmss, toolLabel } from "./layout";
import { recipePrefs } from "./recipePrefs.svelte";
import { vocalTools } from "./vocalTools.svelte";
import { mv } from "./view.svelte";
import { stretchText } from "$lib/state/tempoMatch";

export const errText = (e: unknown) => (typeof e === "string" ? e : e instanceof Error ? e.message : String(e));

export const clipTitle = (c: Clip | undefined) => c?.track?.title || c?.fuzzyId.title || "Unknown track";

/** Words for one selected item, for confirmations and menus: "Clip "Title"", "Reverb on "Title"". */
export function describe(doc: MixDoc, id: string): string {
  const f = E.find(doc, id);
  if (!f) return id;
  if (f.kind === "clip") return `clip "${clipTitle(f.clip)}"`;
  if (f.kind === "process") {
    const p = f.process;
    const name = p.kind === "tool" ? toolLabel(p.effects[0]?.name ?? p.toolType, vocal.mode, isVocalTool(doc.id, p.id)) : toolLabel(p.kind);
    return `${name} on "${clipTitle(f.clip)}"`;
  }
  const t = f.transition;
  const a = t.between ? clipTitle(E.clipOf(doc, t.between.from)) : "?";
  const b = t.between ? clipTitle(E.clipOf(doc, t.between.to)) : "?";
  return `crossfade "${a}" to "${b}"`;
}

/** True when the Wah tool `processId` of mix `mixId` is one the user made as a Vocal tool. */
export const isVocalTool = (mixId: number, processId: string) => vocalTools.has(mixId, processId);

export { toolChoices, type ToolChoice } from "$lib/state/mixedit";

/** The clip a new tool should go on when none is named: the selected one (or the one whose tool is
 * selected), else the one under `at` (default: the playhead), else the first. */
export function targetClip(at: number = mv.playhead): Clip | undefined {
  const doc = mixes.doc;
  if (!doc) return undefined;
  const all = E.sortedClips(doc);
  const sel = all.find((c) => mixes.selected.includes(c.id));
  const selProcessClip = all.find((c) => c.processes.some((p) => mixes.selected.includes(p.id)));
  const under = all.find((c) => at >= c.visibleStart && at < c.visibleEnd);
  return sel ?? selProcessClip ?? under ?? all[0];
}

/** Adds a tool or region on `clip` at timeline `at`. Tools of one kind may not overlap on a
 * clip, so the new one is moved past an existing one; says why when it cannot be placed. */
export function addProcessTo(type: string, clip: Clip | undefined, at: number = mv.playhead, asVocal = false): boolean {
  const doc = mixes.doc;
  if (!doc || !clip) {
    toast.show("Add a track to the mix first.", "info");
    return false;
  }
  const bpm = E.effectiveBpm(clip, Math.min(clip.length, Math.max(0, at - clip.start))) || 120;
  const bar = (60 / bpm) * 4 * SR;
  const isRegion = type === "loop" || type === "ignore" || type === "reverse";
  const want = type === "loop" || type === "reverse" || type === "ignore" ? bar : 4 * bar;
  let start = Math.min(Math.max(clip.trimStart, at - clip.start), clip.length - clip.trimEnd - E.MIN_LENGTH);
  let length = Math.min(want, clip.length - start);
  if (!isRegion) {
    // slide past an existing tool of the same kind
    for (let guard = 0; guard < 50; guard++) {
      const probe = E.newTool(type, start, length);
      const hit = clip.processes.find((p) => p.id === E.toolConflicts(clip, probe)[0]);
      if (!hit) break;
      start = hit.start + hit.length;
      length = Math.min(want, clip.length - start);
      if (length < E.MIN_LENGTH) {
        toast.show(`There is no room for another ${asVocal ? "Vocal" : toolLabel(type, vocal.mode)} on "${clipTitle(clip)}" after the existing one.`, "info");
        return false;
      }
    }
  }
  if (length < E.MIN_LENGTH) {
    toast.show("The clip ends here: move the playhead earlier.", "info");
    return false;
  }
  if (isRegion) mixes.addRegion(clip.id, type, start, length);
  else {
    const tool = mixes.addTool(clip.id, type, start, length, type === "Trans" ? E.effectiveBpm(clip, start) || bpm : 0, asVocal ? "Vocal" : toolLabel(type, vocal.mode));
    if (asVocal) vocalTools.set(doc.id, tool.id, true);
  }
  return true;
}

/** Adds library tracks after the end of the mix (or at `at` for the first), one after another.
 * The decks are not the user's business: overlapping clips are put on different decks by the
 * store. A track that would need more stretch than the setting allows to reach the mix tempo keeps
 * its own tempo, and the toast says so. */
export async function addTracks(ids: number[], at?: number) {
  const doc = mixes.doc;
  if (!doc || ids.length === 0) return;
  let pos = at;
  let added = 0;
  const kept: string[] = [];
  /** Tracks that went somewhere other than the asked-for spot (both decks were busy there). */
  const moved: string[] = [];
  for (const id of ids) {
    try {
      const before = mixes.doc?.clips.length ?? 0;
      const first = before === 0;
      const r = await mixes.addTrack(id, pos, pos === undefined ? before % 2 : undefined);
      added++;
      const cur = mixes.doc;
      if (first && cur && cur.tempoBpm === null && cur.clips[0]) {
        // an empty mix gets its reference tempo from its first track
        const c = cur.clips[0];
        mixes.setTempo(Math.round(E.effectiveBpm(c, c.trimStart) * 100) / 100);
      }
      if (r.choice && !r.matched) kept.push(`${clipTitle(mixes.doc?.clips.find((c) => mixes.selected.includes(c.id)))} (${stretchText(r.choice.stretchPct)} to reach ${r.choice.playsAt.toFixed(1)} BPM)`);
      if (pos !== undefined && mixes.doc) {
        const last = mixes.doc.clips.find((c) => mixes.selected.includes(c.id));
        if (last) {
          if (Math.abs(last.visibleStart - pos) > SR) {
            moved.push(`${clipTitle(last)} at ${mmss(last.visibleStart / SR)}`);
            // show where it went: the clip is not under the playhead
            mv.scrollLeft = Math.max(0, mv.x(last.visibleStart) - mv.viewW * 0.2);
          }
          pos = last.visibleEnd;
        }
      }
    } catch (e) {
      toast.show(`Could not add track: ${errText(e)}`, "bad", 7000);
    }
  }
  if (added > 0) toast.show(added === 1 ? "Track added" : `${added} tracks added`, "ok");
  if (moved.length > 0) toast.show(`Both decks were busy where you asked, so it went later: ${moved.join("; ")}.`, "info", 8000);
  if (kept.length > 0) toast.show(`Kept at its own tempo, since matching would stretch it too much: ${kept.join("; ")}. Use "Sync to mix tempo" in the clip's panel to match anyway.`, "info", 10000);
}

/** Duplicates the selection: clips go right after themselves, a clip's processes after the
 * last one. */
export function duplicateSelection() {
  const doc = mixes.doc;
  if (!doc || mixes.selected.length === 0) return;
  const sel = mixes.selected.map((id) => E.find(doc, id)).filter((f) => f !== null);
  const clips = sel.filter((f) => f.kind === "clip").map((f) => f.clip);
  if (clips.length) {
    const cb = E.copyClips(doc, clips.map((c) => c.id));
    if (!cb || cb.kind !== "clips") return;
    const to = Math.max(...clips.map((c) => c.visibleEnd));
    let ids: string[] = [];
    mixes.edit("Duplicate clips", (d) => {
      const r = E.pasteClips(d, cb, to);
      ids = r.ids;
      return r.doc;
    });
    mixes.select(ids);
    return;
  }
  const procs = sel.filter((f) => f.kind === "process");
  if (procs.length) {
    const clipId = procs[0].clip.id;
    const mine = procs.filter((p) => p.clip.id === clipId).map((p) => p.process);
    const cb = E.copyProcesses(doc, clipId, mine.map((p) => p.id));
    if (!cb || cb.kind !== "processes") return;
    const end = Math.max(...mine.map((p) => p.start + p.length));
    let ids: string[] = [];
    mixes.edit("Duplicate tools", (d) => {
      const r = E.pasteProcesses(d, clipId, cb, end);
      ids = r.ids;
      return r.doc;
    });
    mixes.select(ids);
  }
}

/** Pastes the clipboard at the playhead: clips at the playhead, processes into the clip under
 * it (or the selected / first clip with room). */
export function pasteAtPlayhead() {
  const doc = mixes.doc;
  const cb = mixes.clipboard;
  if (!doc || !cb) return;
  if (cb.kind === "clips") {
    mixes.paste(mv.playhead);
    return;
  }
  const clip = doc.clips.find((c) => mixes.selected.includes(c.id)) ?? E.sortedClips(doc).find((c) => mv.playhead >= c.visibleStart && mv.playhead < c.visibleEnd) ?? E.sortedClips(doc)[0];
  if (!clip) return;
  const sel = doc.clips.find((c) => c.processes.some((p) => mixes.selected.includes(p.id)));
  const target = sel ?? clip;
  mixes.paste(Math.max(target.trimStart, mv.playhead - target.start), target.id);
}

/** Beat-matches a clip to the one before it; reports the outcome (or why not) to the user. */
export async function beatmatch(clipId: string) {
  try {
    const s = await mixes.beatmatch(clipId);
    const bits = [`Matched to ${s.targetBpm.toFixed(1)} BPM`];
    if (s.notes.length) bits.push(s.notes.join("; "));
    toast.show(bits.join(". "), "ok", 7000);
  } catch (e) {
    toast.show(`Beat-match: ${errText(e)}`, "bad", 7000);
  }
}

export type AutoOptions = {
  /** The recipe; default is the one in the settings. */
  recipe?: RecipeId;
  /** Length in bars; default is the settings' (or the recipe's own). */
  bars?: number;
  /** Rebuild this transition: it is deleted in the same undo step. */
  replacing?: string;
};

/** The words for one tool in the way: `EQ on "Title" (overlaps the new one)`. */
function wayText(doc: MixDoc, w: InTheWay): string {
  const clip = E.clipOf(doc, w.clipId);
  const what = `${toolLabel(w.toolType, vocal.mode)} on "${clipTitle(clip)}"`;
  return w.why === "overlaps" ? `${what} (overlaps one the new transition adds)` : `${what} (inside the transition being rebuilt)`;
}

/** Builds a transition between two clips with a recipe (the settings' by default): the backend
 * works out where and how long from the beat grids, the recipe makes the crossfade curve and the
 * tools. Tools of the user's that are in the way are handled by the `onConflict` setting (asks by
 * default, naming every one). One undo step. */
export async function autoTransition(fromId: string, toId: string, opts: AutoOptions = {}) {
  try {
    const prefs = recipePrefs.value;
    const recipe = opts.recipe ?? prefs.recipe;
    const def = recipeDef(recipe);
    const bars = opts.bars ?? requestedBars(recipe, prefs);
    const base = await mixes.autoTransition(fromId, toId, false, bars);
    const doc = mixes.doc;
    if (!doc) return;
    // with the section setting on, the window may move onto section boundaries first (the backend
    // knows nothing of sections); off, or without sections near, this is the backend's own window
    const aligned = alignSuggestion(doc, base, prefs, mixes.sectionLookup, def.lengths[0] * 4 * base.beat);
    const built = buildRecipe(doc, aligned.suggestion, recipe, { cutAt: prefs.cutAt, moved: aligned.moved }, E.recipeKit);
    const old = opts.replacing ? E.find(doc, opts.replacing) : null;
    const replacing = old?.kind === "transition" ? old.transition : null;
    const way = toolsInTheWay(doc, built, replacing);
    const label = `${def.label} (${lengthLabel(built.bars)})`;
    const noteText = built.notes.length ? " " + built.notes.join("; ") + "." : "";

    const finish = (replace: boolean) => {
      recipePrefs.built.set(built.transition.id, { recipe, bars: built.bars });
      mixes.applyTransition(built, replace, {
        label: `Transition: ${label}`,
        replacing: replacing?.id,
        remove: replace ? way.map((w) => w.id) : [],
      });
      const added = describeTools(built, (id) => clipTitle(E.clipOf(doc, id)), (t) => toolLabel(t, vocal.mode));
      // new tools left out because one of the user's is in the way (one old tool can block several)
      const skipped = !replace ? built.tools.filter((t) => t.conflicts.length > 0).length : 0;
      // the rebuilt transition's old tools stay when the user chose to keep theirs
      const kept = !replace ? way.filter((w) => w.why === "inside") : [];
      const bits = [`${label} added.`];
      if (added.length) bits.push(`Tools: ${added.join(", ")}.`);
      if (skipped) bits.push(`${skipped} new tool(s) skipped because yours are in the way.`);
      if (kept.length) bits.push(`Kept (setting "Keep mine"): ${kept.map((w) => wayText(doc, w)).join("; ")}.`);
      if (replace && way.length) bits.push(`Replaced: ${way.map((w) => wayText(doc, w)).join("; ")}.`);
      toast.show(bits.join(" ") + noteText, "ok", 9000);
    };

    if (way.length === 0) return finish(false);
    if (prefs.onConflict === "skip") return finish(false);
    if (prefs.onConflict === "replace") return finish(true);
    confirm.ask({
      title: `Replace tools for the ${def.label.toLowerCase()}?`,
      message: `${way.length === 1 ? "This tool is" : `These ${way.length} tools are`} in the way. Replacing removes ${way.length === 1 ? "it" : "them"} (Undo brings ${way.length === 1 ? "it" : "them"} back). Cancel leaves everything as it is; the setting "When tools are in the way" can be set to keep yours and add only the rest.`,
      items: way.map((w) => wayText(doc, w)),
      confirmLabel: "Replace and build",
      danger: true,
      run: () => finish(true),
    });
  } catch (e) {
    toast.show(`Auto transition: ${errText(e)}`, "bad", 9000);
  }
}

/** The clip a clip should transition into: the next one that overlaps it, else the next one. */
export function neighbours(doc: MixDoc, clipId: string): { prev?: Clip; next?: Clip } {
  const sorted = E.sortedClips(doc);
  const i = sorted.findIndex((c) => c.id === clipId);
  if (i < 0) return {};
  return { prev: sorted[i - 1], next: sorted[i + 1] };
}
