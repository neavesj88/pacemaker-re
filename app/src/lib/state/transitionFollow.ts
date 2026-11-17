// A crossfade and the tools a transition placed follow the clips they sit on. A crossfade is a bare
// window in timeline samples and a tool is clip-local, so nothing ties them together: dragging the
// incoming clip earlier used to leave the fade and the outgoing clip's EQ where they were while the
// incoming clip's EQ rode along. `followTransitions` re-fits them after any edit that moves, trims
// or retimes a clip. It is called from `mixes.edit()` (the one funnel every edit goes through), so
// the live preview, the saved .pmk and the audio export all read the same, already re-fitted
// document. No file format change: only the stored window and the tools' clip-local starts move.
//
// Pure, no Svelte: runs under plain node.
//
// It works from `base`, the document the current undo step started from, on every call. A drag is
// many calls; each one recomputes from the gesture's start, so dragging out and back restores the
// original exactly, nothing accumulates, and the whole drag is one undo step.
//
// What a window was anchored to is read from the geometry of `base` (no stored flag, so mixes
// opened from the Editor or an old file work too), within one bar:
//   both ends (it covered the overlap)  -> it covers the new overlap
//   its end (the usual Auto fade)       -> same length in bars, still ending with the outgoing clip
//   its start                           -> still starting where the incoming clip does, same length
//   neither (placed by hand)            -> stays where it is
// The tools that belong to a transition are the ones on its two clips that lie inside its window
// and touch its start or its end (every tool a recipe makes does); they are re-pinned to the new
// window in proportion. Anything else rides with its clip as before.
// The loops of a Loop out or Loop cut (state/transitionRecipes.ts) are not stretched, a loop's
// slice is whole beats of the outgoing clip: the run of loops that ends at the window's end moves
// with that end, whole and in whole beats of the outgoing clip's grid, so its lengths stay exact
// and every loop still starts on a beat. (A retime of the outgoing clip already carries
// its loops with the audio, see `retempoClip`, so those are left as they are.)
import type { Clip, MixDoc, ProcessDoc, ToolDoc, TransitionDoc } from "../api/mixes";
import { autoWindow } from "./crossfadeLength.ts";
import { BEATS_PER_BAR, MIN_LENGTH, SR, clipBeats, clipOf, effectiveBpm, effectiveFade, refresh, rescaleProcess } from "./mixedit.ts";
import { beatMap, loopRun, type OnMove } from "./transitionRecipes.ts";

type Win = { start: number; length: number };

/** What the clip is called on screen (ClipBlock): its track title; the stored clipName is a generic "AudioClip". */
const nameOf = (c: Clip) => c.track?.title || c.fuzzyId?.title || "a clip";

const samePos = (a: number, b: number) => Math.abs(a - b) <= 1;

/** A clip changed where or how long it is heard, or how fast it plays. */
const moved = (a: Clip, b: Clip) => a.start !== b.start || a.visibleStart !== b.visibleStart || a.visibleEnd !== b.visibleEnd || JSON.stringify(a.tempo) !== JSON.stringify(b.tempo);

/** The edit itself changed this transition (Fit to overlap, a drag of the fade, Rebuild): leave it. */
const touched = (a: TransitionDoc, b: TransitionDoc) => !samePos(a.start, b.start) || !samePos(a.length, b.length) || JSON.stringify(a.curve) !== JSON.stringify(b.curve);

const beatOf = (c: Clip, at: number) => (60 * SR) / Math.max(1, effectiveBpm(c, Math.max(0, at - c.start)));

/** The tools a crossfade placed: on its two clips, inside its window, touching an edge of it (every
 * tool a recipe makes does), plus the run of loops of a loop recipe on the outgoing clip. `claimed`
 * keeps a tool from belonging to two crossfades. */
function membersOf(f: NonNullable<ReturnType<typeof effectiveFade>>, claimed: Set<string>, keepLoops = true) {
  const beat0 = beatOf(f.to, f.start);
  const w0 = { start: f.start, end: f.start + f.length };
  const members: { clip: Clip; p: ToolDoc }[] = [];
  for (const c of [f.from, f.to]) {
    for (const p of c.processes) {
      if (p.kind !== "tool" || claimed.has(p.id)) continue;
      const s = c.start + p.start;
      const e = s + p.length;
      if (s >= w0.start - beat0 && e <= w0.end + beat0 && (Math.abs(s - w0.start) <= beat0 || Math.abs(e - w0.end) <= beat0)) {
        members.push({ clip: c, p });
        claimed.add(p.id);
      }
    }
  }
  const loops = keepLoops ? loopRun(f.from, w0, beat0).filter((p) => !claimed.has(p.id)) : [];
  for (const p of loops) claimed.add(p.id);
  return { members, loops, beat0, w0 };
}

/**
 * Crossfades left with nothing to join: their two clips no longer share any audio (the clips were
 * pulled apart, or one was trimmed or moved away). Each such crossfade is removed from `doc`, with
 * the tools it placed (worked out from `base`, the document the undo step began from, where the
 * crossfade still sat on its clips). Called when a gesture ends or a one-off edit lands, so a drag
 * can still pass through a gap and come back; Undo brings it all back. Returns `doc` itself when
 * nothing is orphaned, and the names of what went.
 */
export function pruneOrphans(base: MixDoc, doc: MixDoc): { doc: MixDoc; removed: string[] } {
  const orphans = doc.transitions.filter((t) => !effectiveFade(doc, t));
  if (orphans.length === 0) return { doc, removed: [] };
  const claimed = new Set<string>();
  const drop = new Set<string>();
  const removed: string[] = [];
  for (const t of orphans) {
    const t0 = base.transitions.find((x) => x.id === t.id);
    const f0 = t0 ? effectiveFade(base, t0) : null;
    if (f0) {
      const { members, loops } = membersOf(f0, claimed);
      for (const { p } of members) drop.add(p.id);
      for (const p of loops) drop.add(p.id);
      removed.push(`the crossfade from "${nameOf(f0.from)}" into "${nameOf(f0.to)}"` + (members.length + loops.length > 0 ? ` and its ${members.length + loops.length} tool${members.length + loops.length === 1 ? "" : "s"}` : ""));
    } else removed.push(`the crossfade "${t.name}"`);
  }
  const gone = new Set(orphans.map((t) => t.id));
  const clips = doc.clips.map((c) => (c.processes.some((p) => drop.has(p.id)) ? { ...c, processes: c.processes.filter((p) => !drop.has(p.id)) } : c));
  return { doc: refresh({ ...doc, clips, transitions: doc.transitions.filter((t) => !gone.has(t.id)) }), removed };
}

/** `after` (the document `fn` produced from `before`) with the crossfades whose clips moved since
 * `base` re-fitted, and their tools with them. `base` is the document the current undo step began
 * from (`before` when the edit starts a new step). "stay" and "nothing moved" return `after` itself. */
export function followTransitions(base: MixDoc, after: MixDoc, mode: OnMove, before: MixDoc = base): MixDoc {
  if (mode !== "follow" || base === after || base.transitions.length === 0) return after;
  const claimed = new Set<string>();
  const newT = new Map<string, Win>();
  const newP = new Map<string, Map<string, ProcessDoc>>(); // clip id -> process id -> replacement
  const put = (clipId: string, p: ProcessDoc) => newP.set(clipId, (newP.get(clipId) ?? new Map<string, ProcessDoc>()).set(p.id, p));

  for (const t0 of [...base.transitions].sort((a, b) => a.start - b.start)) {
    const t1 = after.transitions.find((x) => x.id === t0.id);
    const tb = before.transitions.find((x) => x.id === t0.id);
    if (!t1 || !tb || touched(tb, t1)) continue;
    const f = effectiveFade(base, t0);
    if (!f) continue;
    const from1 = clipOf(after, f.from.id);
    const to1 = clipOf(after, f.to.id);
    if (!from1 || !to1) continue;

    // the tools this transition placed, and the loops of a loop recipe (outgoing clip only; not when
    // its tempo changed, see the header)
    const { members, loops: loopMembers, beat0, w0 } = membersOf(f, claimed, JSON.stringify(f.from.tempo) === JSON.stringify(from1.tempo));
    const bar0 = beat0 * BEATS_PER_BAR;

    // only an edit that moved, trimmed or retimed one of its clips re-fits it. Compared with the
    // previous frame, not with `base`: a merged step that edits a tool, an envelope or the fade
    // itself would otherwise be rebuilt from `base` and snap back on every frame after the first.
    // A drag that came back still moved a clip relative to `before`, so it is restored from `base`.
    const fromB = clipOf(before, f.from.id);
    const toB = clipOf(before, f.to.id);
    if (!fromB || !toB || (!moved(fromB, from1) && !moved(toB, to1))) continue;

    const lo0 = Math.max(f.from.visibleStart, f.to.visibleStart);
    const end0 = Math.min(f.from.visibleEnd, f.to.visibleEnd);
    const lo1 = Math.max(from1.visibleStart, to1.visibleStart);
    const end1 = Math.min(from1.visibleEnd, to1.visibleEnd);
    const startAnchored = f.start - lo0 <= bar0 + 1;
    const endAnchored = end0 - w0.end <= bar0 + 1;
    const beat1 = beatOf(to1, lo1);
    const bar1 = beat1 * BEATS_PER_BAR;

    let w1: Win | null = null;
    if (to1.visibleStart > from1.visibleStart && end1 > lo1 && beat1 > 0) {
      if (!startAnchored && !endAnchored) w1 = { start: f.start, length: f.length };
      else if (startAnchored && endAnchored) w1 = autoWindow(to1, lo1, end1, beat1, (end1 - lo1) / bar1);
      else if (endAnchored) w1 = autoWindow(to1, lo1, end1, beat1, f.length / bar0);
      else {
        const start = Math.round(lo1 + (f.start - lo0));
        const length = Math.min(Math.round((f.length / bar0) * bar1), Math.round(end1 - start));
        w1 = length >= beat1 * 0.5 ? { start, length } : null;
      }
    }

    if (!w1) {
      // the clips no longer overlap (or swapped places): the crossfade is inert, as the mixer treats
      // it; keep it and its tools as they were when the gesture began, so dragging back restores them
      newT.set(t0.id, { start: t0.start, length: t0.length });
      for (const { clip, p } of members) {
        const c1 = clipOf(after, clip.id);
        if (c1 && JSON.stringify(c1.tempo) === JSON.stringify(clip.tempo)) put(clip.id, p);
      }
      for (const p of loopMembers) put(f.from.id, p);
      continue;
    }

    const same = samePos(w1.start, f.start) && samePos(w1.length, f.length);
    // an unchanged window keeps what was stored (it may reach past the shared audio and be cut when heard)
    newT.set(t0.id, same ? { start: t0.start, length: t0.length } : w1);
    const ratio = samePos(w1.length, f.length) ? 1 : w1.length / f.length;
    const anchor = same ? f.start : w1.start;
    for (const { clip, p } of members) {
      const c1 = clipOf(after, clip.id);
      if (!c1) continue;
      const abs = anchor + (clip.start + p.start - f.start) * ratio;
      const length = Math.min(c1.length, Math.max(MIN_LENGTH, Math.round(ratio === 1 ? p.length : p.length * ratio)));
      const start = Math.min(Math.max(0, Math.round(abs) - c1.start), c1.length - length);
      put(clip.id, rescaleProcess({ ...p, start, length }, p.length));
    }
    // the run of loops keeps its shape and its place on the outgoing clip's beat grid; it moves, in
    // whole beats, when the end of the window has moved by more than the clip itself did
    if (loopMembers.length > 0) {
      const wanted = same ? 0 : w1.start + w1.length - (f.start + f.length);
      const k = Math.round((wanted - (from1.start - f.from.start)) / beatOf(from1, w1.start + w1.length));
      let delta = 0;
      if (k !== 0) {
        const at = from1.start + loopMembers[0].start;
        const beats = clipBeats(from1);
        if (beats.length >= 2) {
          const map = beatMap(beats);
          delta = Math.round(map.time(map.pos(at) + k)) - at;
        } else delta = Math.round(k * beatOf(from1, w1.start + w1.length));
      }
      for (const p of loopMembers) {
        const start = Math.min(Math.max(0, p.start + delta), Math.max(0, from1.length - p.length));
        put(f.from.id, rescaleProcess({ ...p, start }, p.length));
      }
    }
  }

  let changed = false;
  const transitions = after.transitions.map((t) => {
    const w = newT.get(t.id);
    if (!w || (samePos(w.start, t.start) && samePos(w.length, t.length))) return t;
    changed = true;
    return { ...t, start: w.start, length: w.length };
  });
  const clips = after.clips.map((c) => {
    const m = newP.get(c.id);
    if (!m) return c;
    let any = false;
    const processes = c.processes.map((p) => {
      const q = m.get(p.id);
      if (!q) return p;
      if (JSON.stringify(q) === JSON.stringify(p)) return p;
      any = true;
      return q;
    });
    if (!any) return c;
    changed = true;
    return { ...c, processes };
  });
  return changed ? refresh({ ...after, clips, transitions }) : after;
}
