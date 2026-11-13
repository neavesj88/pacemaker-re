// Pure editing logic for the mix editor: no Svelte, no Tauri, no DOM, so it runs under plain
// node (app/scripts/test-mixedit.mjs) and is shared by the store (mixes.svelte.ts) and the
// timeline UI. Every function takes a `MixDoc` and returns a new one (structural sharing: the
// beat grids and untouched clips are the same objects), so the store can keep undo snapshots
// cheaply. The model and its units are documented in api/mixes.ts; read its header first.
//
// Contents: ids, TempoMap (port of pm_formats::mix::TempoMap), derived fields, beats and
// snapping, clip / process / transition edits, envelopes, tempo changes (retempo, beat-match,
// auto-transition application), clipboard, tool catalogue.
import type {
  BeatmatchSuggestion,
  Clip,
  CurveNode,
  EnvNode,
  LoopDoc,
  MixDoc,
  ProcessDoc,
  Span,
  TempoNode,
  ToolDoc,
  ToolType,
  TransitionDoc,
  TransitionSuggestion,
} from "../api/mixes";
import type { SectionKind, SectionOut } from "../api/analysis";
import type { VocalMode } from "../api/device";
import type { Kit } from "./transitionRecipes";

/** Engine sample rate (same as SAMPLE_RATE in api/mixes.ts; duplicated so this file imports no value). */
export const SR = 44100;
export const BEATS_PER_BAR = 4;
/** A phrase is 8 bars (32 beats), the usual unit DJs line transitions up on. */
export const PHRASE_BARS = 8;
/** Shortest audible clip or process an edit may leave, in samples (0.05 s). */
export const MIN_LENGTH = 2205;

/** "section": snap to the boundaries of detected sections (intro / build / drop / ...) where a
 * clip has them, and to its phrase lines where it has none, so tracks without sections snap as in
 * "phrase". */
export type SnapMode = "off" | "beat" | "bar" | "phrase" | "section";

const secOf = (samples: number) => samples / SR;

// ---------------------------------------------------------------------------------------------
// Ids
// ---------------------------------------------------------------------------------------------

/** A fresh id: 32 lowercase hex characters, the shape of the Editor's SessionEventId. */
export function newId(): string {
  const c = (globalThis as { crypto?: { randomUUID?: () => string } }).crypto;
  if (c?.randomUUID) return c.randomUUID().replace(/-/g, "");
  let s = "";
  for (let i = 0; i < 32; i++) s += Math.floor(Math.random() * 16).toString(16);
  return s;
}

// ---------------------------------------------------------------------------------------------
// Tempo curve
// ---------------------------------------------------------------------------------------------

/** Port of pm_formats::mix::TempoMap: maps clip-local samples to source samples (the unstretched
 * file at 44.1 kHz) and back. The curve is piecewise linear in `speed`, flat outside its nodes. */
export class TempoMap {
  pos: number[];
  speed: number[];
  cum: number[];

  constructor(nodes: TempoNode[]) {
    const v = nodes.map((n) => [n.pos, Number.isFinite(n.speed) && n.speed > 0 ? n.speed : 1] as [number, number]);
    if (v.length === 0) v.push([0, 1]);
    v.sort((a, b) => a[0] - b[0]);
    this.pos = v.map((n) => n[0]);
    this.speed = v.map((n) => n[1]);
    this.cum = [this.speed[0] * this.pos[0]];
    for (let i = 1; i < this.pos.length; i++) {
      this.cum.push(this.cum[i - 1] + 0.5 * (this.speed[i - 1] + this.speed[i]) * (this.pos[i] - this.pos[i - 1]));
    }
  }

  /** Number of entries of `a` (sorted) that satisfy `a[i] <= x`. */
  private static upper(a: number[], x: number): number {
    let lo = 0;
    let hi = a.length;
    while (lo < hi) {
      const mid = (lo + hi) >>> 1;
      if (a[mid] <= x) lo = mid + 1;
      else hi = mid;
    }
    return lo;
  }

  speedAt(local: number): number {
    if (local <= this.pos[0]) return this.speed[0];
    const i = TempoMap.upper(this.pos, local) - 1;
    if (i === this.pos.length - 1) return this.speed[i];
    const t = (local - this.pos[i]) / (this.pos[i + 1] - this.pos[i]);
    return this.speed[i] + (this.speed[i + 1] - this.speed[i]) * t;
  }

  /** Source position played at clip-local `local`. */
  sourceAt(local: number): number {
    if (local <= this.pos[0]) return this.speed[0] * local;
    const i = TempoMap.upper(this.pos, local) - 1;
    const dx = local - this.pos[i];
    if (i === this.pos.length - 1) return this.cum[i] + this.speed[i] * dx;
    const l = this.pos[i + 1] - this.pos[i];
    return this.cum[i] + this.speed[i] * dx + ((this.speed[i + 1] - this.speed[i]) / (2 * l)) * dx * dx;
  }

  /** Clip-local position at which source position `source` is played. */
  localAt(source: number): number {
    if (source <= this.cum[0]) return source / this.speed[0];
    const i = TempoMap.upper(this.cum, source) - 1;
    const d = source - this.cum[i];
    if (i === this.pos.length - 1) return this.pos[i] + d / this.speed[i];
    const l = this.pos[i + 1] - this.pos[i];
    const s0 = this.speed[i];
    const a = (this.speed[i + 1] - s0) / (2 * l);
    const disc = Math.max(0, s0 * s0 + 4 * a * d);
    return this.pos[i] + (2 * d) / (s0 + Math.sqrt(disc));
  }
}

/** Effective tempo (BPM) of a clip at a clip-local position. */
export const effectiveBpm = (c: Clip, local: number): number => c.bpm * new TempoMap(c.tempo).speedAt(local);

// ---------------------------------------------------------------------------------------------
// Derived fields
// ---------------------------------------------------------------------------------------------

const withSec = <T extends Span>(s: T): T => ({ ...s, startSec: secOf(s.start), lengthSec: secOf(s.length) });

/** Recomputes the derived fields of one clip (seconds, visible range, tool types). */
export function refreshClip(c: Clip): Clip {
  const out = withSec(c);
  out.visibleStart = c.start + c.trimStart;
  out.visibleEnd = c.start + c.length - c.trimEnd;
  out.visibleStartSec = secOf(out.visibleStart);
  out.visibleEndSec = secOf(out.visibleEnd);
  out.processes = c.processes.map((p) => {
    const q = withSec(p);
    if (q.kind === "tool") q.toolType = q.effects[0]?.name ?? "";
    return q;
  });
  out.volume = withSec(c.volume);
  return out;
}

/** The `between` links by geometry (port of the backend rule): the clip whose audible start is
 * nearest the transition's start, and the earlier clip overlapping the transition most. */
export function linkTransitions(doc: MixDoc): MixDoc {
  const transitions = doc.transitions.map((t) => {
    const a = t.start;
    const b = t.start + t.length;
    let to: Clip | null = null;
    for (const c of doc.clips) {
      const d = Math.abs(c.visibleStart - a);
      if (d <= Math.max(t.length, SR) && (to === null || d < Math.abs(to.visibleStart - a))) to = c;
    }
    let from: Clip | null = null;
    let best = 0;
    if (to) {
      for (const c of doc.clips) {
        if (c.id === to.id || c.visibleStart >= to.visibleStart) continue;
        const o = Math.max(0, Math.min(c.visibleEnd, b) - Math.max(c.visibleStart, a));
        if (o > best) {
          best = o;
          from = c;
        }
      }
    }
    return { ...withSec(t), between: to && from ? { from: from.id, to: to.id } : null };
  });
  return { ...doc, transitions };
}

/** Recomputes every derived field of the document and re-links the transitions. Call it after
 * any edit that is not one of the functions below (they all do). */
export function refresh(doc: MixDoc): MixDoc {
  const clips = doc.clips.map(refreshClip);
  const length = clips.reduce((m, c) => Math.max(m, c.visibleEnd), 0);
  return linkTransitions({ ...doc, clips, length, lengthSec: secOf(length) });
}

// ---------------------------------------------------------------------------------------------
// Lookup
// ---------------------------------------------------------------------------------------------

export type Found =
  | { kind: "clip"; clip: Clip }
  | { kind: "process"; clip: Clip; process: ProcessDoc }
  | { kind: "transition"; transition: TransitionDoc };

/** The clip, process or transition with this id. */
export function find(doc: MixDoc, id: string): Found | null {
  for (const c of doc.clips) {
    if (c.id === id) return { kind: "clip", clip: c };
    const p = c.processes.find((x) => x.id === id);
    if (p) return { kind: "process", clip: c, process: p };
  }
  const t = doc.transitions.find((x) => x.id === id);
  return t ? { kind: "transition", transition: t } : null;
}

export const clipOf = (doc: MixDoc, id: string): Clip | undefined => doc.clips.find((c) => c.id === id);

/** Clips in timeline order (audible start). */
export const sortedClips = (doc: MixDoc): Clip[] => [...doc.clips].sort((a, b) => a.visibleStart - b.visibleStart);

export const localToTimeline = (c: Clip, local: number): number => c.start + local;
export const timelineToLocal = (c: Clip, t: number): number => t - c.start;

// ---------------------------------------------------------------------------------------------
// Beats and snapping
// ---------------------------------------------------------------------------------------------

export type BeatKind = "beat" | "bar" | "phrase";
export type BeatPoint = { /** timeline samples */ time: number; /** index into the grid */ index: number; kind: BeatKind };

/** Every beat of a clip's grid on the timeline, classed by position relative to the first
 * downbeat (bar = every 4th beat, phrase = every 32nd). Without a downbeat the first beat of the
 * grid counts as one. Empty without a grid. Beats outside the audible range are included. */
export function clipBeatPoints(c: Clip): BeatPoint[] {
  const g = c.track?.grid;
  if (!g || g.beatFrames.length === 0) return [];
  const tm = new TempoMap(c.tempo);
  const k = SR / Math.max(1, g.sourceRate);
  const fd = g.firstDownbeat ?? 0;
  const bar = BEATS_PER_BAR;
  const phrase = BEATS_PER_BAR * PHRASE_BARS;
  return g.beatFrames.map((f, index) => {
    const rel = (((index - fd) % phrase) + phrase) % phrase;
    const kind: BeatKind = rel === 0 ? "phrase" : rel % bar === 0 ? "bar" : "beat";
    return { time: c.start + tm.localAt(f * k), index, kind };
  });
}

/** Timeline samples of the beats only (see `clipBeatPoints`). */
export const clipBeats = (c: Clip): number[] => clipBeatPoints(c).map((b) => b.time);

const rank: Record<BeatKind, number> = { beat: 0, bar: 1, phrase: 2 };
const wanted: Record<Exclude<SnapMode, "off">, number> = { beat: 0, bar: 1, phrase: 2, section: 2 };

// ---- sections (app-side phrase detection, see state/sections.svelte.ts) -------------------------

/** A track's sections by track id, or null when it has none. The store passes `sections.of`; this
 * file takes it as a function so it stays free of stores. */
export type SectionsLookup = (trackId: number) => readonly SectionOut[] | null;

/** A boundary between two sections of a clip, placed on the timeline. */
export type SectionPoint = {
  /** Timeline samples. */
  time: number;
  before: SectionKind;
  after: SectionKind;
  /** How strong the boundary is, 0 to 1. */
  confidence: number;
};

/** The boundaries of a clip's sections on the timeline (the start of every section but the
 * first), ascending. Section times are seconds in the beat grid's time base, i.e. source samples
 * at `SR`, so the clip's tempo curve carries them to the timeline exactly as it does its beats.
 * All of them are returned, including those outside the audible range. */
export function clipSectionPoints(c: Clip, sections: readonly SectionOut[] | null | undefined): SectionPoint[] {
  if (!sections || sections.length < 2) return [];
  const tm = new TempoMap(c.tempo);
  const out: SectionPoint[] = [];
  for (let i = 1; i < sections.length; i++) {
    const s = sections[i];
    if (!Number.isFinite(s.start_secs)) continue;
    out.push({ time: c.start + tm.localAt(s.start_secs * SR), before: sections[i - 1].kind, after: s.kind, confidence: s.start_confidence });
  }
  return out.sort((a, b) => a.time - b.time);
}

/** One section of a clip in clip-local samples (before the clip's `start` is added). */
export type SectionSpan = { from: number; to: number; kind: SectionKind; confidence: number };

/** The sections of a clip as clip-local sample ranges, for drawing on its waveform. */
export function clipSectionSpans(c: Clip, sections: readonly SectionOut[] | null | undefined): SectionSpan[] {
  if (!sections || sections.length === 0) return [];
  const tm = new TempoMap(c.tempo);
  return sections
    .filter((s) => Number.isFinite(s.start_secs) && Number.isFinite(s.end_secs) && s.end_secs > s.start_secs)
    .map((s) => ({ from: tm.localAt(s.start_secs * SR), to: tm.localAt(s.end_secs * SR), kind: s.kind, confidence: s.confidence }));
}

/** The points of a clip that a mode snaps to, on the timeline and inside its audible range: beats
 * of the mode's class or higher, or, in "section" mode, its section boundaries (its phrase lines
 * where it has no sections). */
function snapPointsOf(c: Clip, mode: Exclude<SnapMode, "off">, sections?: SectionsLookup): number[] {
  if (mode === "section") {
    const pts = clipSectionPoints(c, sections?.(c.trackId));
    if (pts.length > 0) return pts.map((p) => p.time).filter((t) => t >= c.visibleStart - 1 && t <= c.visibleEnd + 1);
  }
  const min = wanted[mode];
  return clipBeatPoints(c)
    .filter((b) => rank[b.kind] >= min && b.time >= c.visibleStart - 1 && b.time <= c.visibleEnd + 1)
    .map((b) => b.time);
}

/** Evenly spaced lines of the reference tempo from timeline 0 (beat / bar / phrase), for the
 * ruler and as snap targets where no clip has a grid. */
export function uniformGrid(bpm: number, mode: Exclude<SnapMode, "off">, from: number, to: number): number[] {
  if (!(bpm > 0)) return [];
  const beat = (60 / bpm) * SR;
  const step = beat * (mode === "beat" ? 1 : mode === "bar" ? BEATS_PER_BAR : BEATS_PER_BAR * PHRASE_BARS);
  const out: number[] = [];
  for (let t = Math.max(0, Math.ceil(from / step)) * step; t <= to; t += step) out.push(t);
  return out;
}

export type SnapOptions = {
  mode: SnapMode;
  /** Snap distance in samples (pixels per sample is the view's business). */
  threshold: number;
  /** Clip ids that are being dragged and must not be snap targets. */
  exclude?: string[];
  /** Where to find a track's sections; used by the "section" mode only. */
  sections?: SectionsLookup;
};

/** Sorted snap targets: the audible beats of the other clips at or above the mode's class (their
 * section boundaries in "section" mode), or the reference tempo's grid when no other clip has
 * one. */
export function snapTargets(doc: MixDoc, mode: SnapMode, exclude: string[] = [], range: [number, number] | null = null, sections?: SectionsLookup): number[] {
  if (mode === "off") return [];
  const out: number[] = [];
  for (const c of doc.clips) {
    if (exclude.includes(c.id)) continue;
    out.push(...snapPointsOf(c, mode, sections));
  }
  if (out.length === 0 && doc.tempoBpm) {
    const [from, to] = range ?? [0, Math.max(doc.length, 60 * SR)];
    out.push(...uniformGrid(doc.tempoBpm, mode, from, to));
  }
  return out.sort((a, b) => a - b);
}

/** Index of the entry of `sorted` nearest to `x`, or -1. */
export function nearestIndex(sorted: number[], x: number): number {
  if (sorted.length === 0) return -1;
  let lo = 0;
  let hi = sorted.length;
  while (lo < hi) {
    const mid = (lo + hi) >>> 1;
    if (sorted[mid] < x) lo = mid + 1;
    else hi = mid;
  }
  if (lo === 0) return 0;
  if (lo === sorted.length) return sorted.length - 1;
  return x - sorted[lo - 1] <= sorted[lo] - x ? lo - 1 : lo;
}

/** `x` moved onto the nearest target if one is within `threshold`, else `x`. */
export function snap(x: number, targets: number[], threshold: number): number {
  const i = nearestIndex(targets, x);
  return i >= 0 && Math.abs(targets[i] - x) <= threshold ? targets[i] : x;
}

/** Where a dragged clip should start so that its beats (of the mode's class or higher, and its
 * audible start) land on a snap target: the smallest correction within the threshold, else
 * `newStart` unchanged. */
export function snapClipStart(doc: MixDoc, clipId: string, newStart: number, opts: SnapOptions): number {
  const c = clipOf(doc, clipId);
  if (!c || opts.mode === "off") return newStart;
  const exclude = [clipId, ...(opts.exclude ?? [])];
  const targets = snapTargets(doc, opts.mode, exclude, [newStart, newStart + c.length], opts.sections);
  if (targets.length === 0) return newStart;
  // the dragged clip's own points, relative to its start: audible beats (section boundaries in
  // "section" mode) and the audible start
  const own = snapPointsOf(c, opts.mode, opts.sections).map((t) => t - c.start);
  own.push(c.trimStart);
  let best = Infinity;
  for (const rel of own) {
    const i = nearestIndex(targets, newStart + rel);
    if (i < 0) continue;
    const d = targets[i] - (newStart + rel);
    if (Math.abs(d) < Math.abs(best)) best = d;
  }
  return Math.abs(best) <= opts.threshold ? newStart + best : newStart;
}

// ---------------------------------------------------------------------------------------------
// Clip edits
// ---------------------------------------------------------------------------------------------

const replaceClip = (doc: MixDoc, id: string, fn: (c: Clip) => Clip): MixDoc =>
  refresh({ ...doc, clips: doc.clips.map((c) => (c.id === id ? fn(c) : c)) });

/** Applies `fn` to one clip and refreshes the derived fields. */
export const updateClip = replaceClip;

/** Moves a clip (its `start`) to `newStart`, snapped when `opts` asks for it, never before
 * timeline 0 for its audible start. */
export function moveClip(doc: MixDoc, id: string, newStart: number, opts?: SnapOptions & { channel?: number }): MixDoc {
  const c = clipOf(doc, id);
  if (!c) return doc;
  let s = opts ? snapClipStart(doc, id, newStart, opts) : newStart;
  s = Math.max(s, -c.trimStart);
  return replaceClip(doc, id, (x) => ({ ...x, start: s, channel: opts?.channel ?? x.channel }));
}

/** The deck (0 or 1) that has room for an audible range, else the one with the smaller overlap. */
export function freeChannel(doc: MixDoc, from: number, to: number, exclude: string[] = []): number {
  const overlap = [0, 0];
  for (const c of doc.clips) {
    if (exclude.includes(c.id)) continue;
    const o = Math.max(0, Math.min(c.visibleEnd, to) - Math.max(c.visibleStart, from));
    overlap[c.channel === 1 ? 1 : 0] += o;
  }
  return overlap[1] < overlap[0] ? 1 : 0;
}

/** Trims a clip from an edge to timeline position `pos` (snapped like a move): the audible
 * start / end moves, the audio and everything on it stay where they are. Cannot reveal audio
 * that does not exist (before the file's start or after its end) or shrink below `MIN_LENGTH`. */
export function resizeClip(doc: MixDoc, id: string, edge: "left" | "right", pos: number, opts?: SnapOptions): MixDoc {
  const c = clipOf(doc, id);
  if (!c) return doc;
  const targets = opts && opts.mode !== "off" ? snapTargets(doc, opts.mode, [id, ...(opts.exclude ?? [])], [pos - SR, pos + SR], opts.sections) : [];
  const p = opts ? snap(pos, targets, opts.threshold) : pos;
  if (edge === "left") {
    const v = Math.min(Math.max(p, c.start), c.visibleEnd - MIN_LENGTH);
    return replaceClip(doc, id, (x) => ({ ...x, trimStart: v - x.start }));
  }
  const v = Math.max(Math.min(p, c.start + c.length), c.visibleStart + MIN_LENGTH);
  return replaceClip(doc, id, (x) => ({ ...x, trimEnd: x.start + x.length - v }));
}

/** Adds a clip so that its audible start is at `start`, on `channel` or the free deck. */
export function addClip(doc: MixDoc, clip: Clip, start: number, channel?: number): MixDoc {
  const placed = { ...clip, start: start - clip.trimStart };
  const vs = start;
  const ve = start + clip.length - clip.trimStart - clip.trimEnd;
  placed.channel = channel ?? freeChannel(doc, vs, ve);
  return refresh({ ...doc, clips: [...doc.clips, placed] });
}

/** Deletes clips and the transitions that joined them. */
export function deleteClips(doc: MixDoc, ids: string[]): MixDoc {
  const gone = new Set(ids);
  const transitions = doc.transitions.filter((t) => !(t.between && (gone.has(t.between.from) || gone.has(t.between.to))));
  return refresh({ ...doc, clips: doc.clips.filter((c) => !gone.has(c.id)), transitions });
}

// ---------------------------------------------------------------------------------------------
// Processes (tools, loops, ignore, reverse): clip-local ranges
// ---------------------------------------------------------------------------------------------

/** A flat tool of a catalogue type (`TOOL_TYPES`, or Trans) over clip-local `[start, start + length)`.
 * `startBpm` is only used by Trans: the tempo its gate follows (the clip's tempo where the tool
 * starts); 0 leaves it to the player to use the clip's tempo. */
export function newTool(type: string, start: number, length: number, id: string = newId(), startBpm = 0): ToolDoc {
  const t = [...TOOL_TYPES, TRANS_TYPE].find((x) => x.name === type);
  if (!t) throw new Error(`unknown tool type ${type}`);
  const last = Math.max(1, Math.round(length) - 1);
  const params: Record<string, EnvNode[]> = {};
  for (const p of t.params) params[p.key] = [{ pos: 0, value: p.flat }, { pos: last, value: p.flat }];
  const cls = type === "Trans" ? { class: "trans", startBpm: Math.max(0, Math.round(startBpm * 100) / 100), active: true } : EFFECT_CLASS[type];
  return {
    kind: "tool",
    id,
    toolType: type,
    name: "ToolProcess",
    bypassed: false,
    effects: [{ name: type, bypassed: false, ...cls }],
    params,
    start,
    length,
    trimStart: 0,
    trimEnd: 0,
    limitSize: false,
    startSec: secOf(start),
    lengthSec: secOf(length),
  } as ToolDoc;
}

/** A loop, ignore or reverse region over clip-local `[start, start + length)`. */
export function newRegion(kind: "loop" | "ignore" | "reverse", start: number, length: number, id: string = newId()): ProcessDoc {
  const base = { id, start, length, trimStart: 0, trimEnd: 0, limitSize: false, startSec: secOf(start), lengthSec: secOf(length), bypassed: false };
  switch (kind) {
    case "loop":
      return { ...base, kind, name: "LOOP", iterationLength: length / 2, iterations: 2, reLoop: false, mode: false, parent: null, offsets: [] };
    case "ignore":
      return { ...base, kind, name: "IGNORE", startPos: Math.max(0, Math.round(start)) };
    case "reverse":
      return { ...base, kind, name: "REVERSE" };
  }
}

/** Other tools of the same kind that overlap `tool` (the Editor allows none; saving refuses). */
export function toolConflicts(clip: Clip, tool: ToolDoc): string[] {
  return clip.processes
    .filter((p): p is ToolDoc => p.kind === "tool" && p.id !== tool.id && p.effects[0]?.name === tool.effects[0]?.name)
    .filter((p) => p.start < tool.start + tool.length - 1 && tool.start < p.start + p.length - 1)
    .map((p) => p.id);
}

/** Appends a process to a clip (its range is clamped into the clip). */
export function addProcess(doc: MixDoc, clipId: string, p: ProcessDoc): MixDoc {
  return replaceClip(doc, clipId, (c) => {
    const start = Math.min(Math.max(0, p.start), Math.max(0, c.length - MIN_LENGTH));
    const length = Math.min(p.length, c.length - start);
    return { ...c, processes: [...c.processes, rescaleProcess({ ...p, start, length })] };
  });
}

export function updateProcess(doc: MixDoc, clipId: string, id: string, fn: (p: ProcessDoc) => ProcessDoc): MixDoc {
  return replaceClip(doc, clipId, (c) => ({ ...c, processes: c.processes.map((p) => (p.id === id ? fn(p) : p)) }));
}

/** Moves a process to clip-local `newStart` (snapped on the timeline when asked), keeping it
 * inside its clip. */
export function moveProcess(doc: MixDoc, clipId: string, id: string, newStart: number, opts?: SnapOptions): MixDoc {
  const c = clipOf(doc, clipId);
  const p = c?.processes.find((x) => x.id === id);
  if (!c || !p) return doc;
  let s = newStart;
  if (opts && opts.mode !== "off") {
    const targets = snapTargets(doc, opts.mode, opts.exclude ?? [], [c.start + s - SR, c.start + s + SR], opts.sections);
    // snap the start, or the end if that is nearer
    const a = snap(c.start + s, targets, opts.threshold) - c.start;
    const b = snap(c.start + s + p.length, targets, opts.threshold) - c.start - p.length;
    s = Math.abs(a - s) <= Math.abs(b - s) ? a : b;
  }
  s = Math.min(Math.max(0, s), c.length - p.length);
  return updateProcess(doc, clipId, id, (x) => ({ ...x, start: s }));
}

/** Resizes a process from an edge to clip-local `pos`. A tool's envelopes stretch with it. */
export function resizeProcess(doc: MixDoc, clipId: string, id: string, edge: "left" | "right", pos: number, opts?: SnapOptions): MixDoc {
  const c = clipOf(doc, clipId);
  const p = c?.processes.find((x) => x.id === id);
  if (!c || !p) return doc;
  let q = pos;
  if (opts && opts.mode !== "off") {
    q = snap(c.start + pos, snapTargets(doc, opts.mode, opts.exclude ?? [], [c.start + pos - SR, c.start + pos + SR], opts.sections), opts.threshold) - c.start;
  }
  const end = p.start + p.length;
  const [start, stop] =
    edge === "left" ? [Math.min(Math.max(0, q), end - MIN_LENGTH), end] : [p.start, Math.max(Math.min(c.length, q), p.start + MIN_LENGTH)];
  return updateProcess(doc, clipId, id, (x) => rescaleProcess({ ...x, start, length: stop - start }, x.length));
}

/** Rescales a tool's envelope positions after its length changed from `oldLength` (default:
 * unchanged, only the last-node and derived fields are normalised). */
function rescaleProcess(p: ProcessDoc, oldLength: number = p.length): ProcessDoc {
  const q = withSec(p);
  if (q.kind !== "tool") return q;
  const last = Math.max(0, Math.round(p.length) - 1);
  // endpoints map onto endpoints: the last node of a tool sits at length - 1
  const oldLast = Math.round(oldLength) - 1;
  const r = oldLast > 0 ? last / oldLast : 1;
  const params: Record<string, EnvNode[]> = {};
  for (const [k, nodes] of Object.entries(q.params)) {
    params[k] = nodes.map((n) => ({ pos: Math.min(last, Math.round(n.pos * r)), value: n.value }));
  }
  return { ...q, params };
}

/** Deletes processes by id (from whichever clip holds them). A loop that continued a deleted
 * loop becomes a first loop (`parent` null). */
export function deleteProcesses(doc: MixDoc, ids: string[]): MixDoc {
  const gone = new Set(ids);
  return refresh({
    ...doc,
    clips: doc.clips.map((c) =>
      c.processes.some((p) => gone.has(p.id))
        ? {
            ...c,
            processes: c.processes
              .filter((p) => !gone.has(p.id))
              .map((p) => (p.kind === "loop" && p.parent && gone.has(p.parent) ? ({ ...p, parent: null } as LoopDoc) : p)),
          }
        : c,
    ),
  });
}

// ---------------------------------------------------------------------------------------------
// Envelopes
// ---------------------------------------------------------------------------------------------

/** Which envelope of a clip: its gain, tempo or key curve, or one parameter of a tool. */
export type EnvelopeTarget =
  | { kind: "gain" }
  | { kind: "tempo" }
  | { kind: "key" }
  | { kind: "param"; processId: string; key: string };

/** Inserts a node keeping `pos` order; a node at the same `pos` is replaced. */
export function insertNode<T extends { pos: number }>(nodes: T[], node: T): T[] {
  const i = nodes.findIndex((n) => n.pos >= node.pos);
  if (i < 0) return [...nodes, node];
  if (nodes[i].pos === node.pos) return nodes.map((n, j) => (j === i ? node : n));
  return [...nodes.slice(0, i), node, ...nodes.slice(i)];
}

/** Moves node `i` to a new position and value, between its neighbours (positions are integers). */
export function moveNode<T extends { pos: number }>(nodes: T[], i: number, change: Partial<T>): T[] {
  const lo = i > 0 ? nodes[i - 1].pos : 0;
  const hi = i < nodes.length - 1 ? nodes[i + 1].pos : Infinity;
  return nodes.map((n, j) => {
    if (j !== i) return n;
    const next = { ...n, ...change };
    return { ...next, pos: Math.round(Math.min(Math.max(next.pos, lo), hi)) };
  });
}

export const removeNode = <T,>(nodes: T[], i: number): T[] => nodes.filter((_, j) => j !== i);

/** The value of a piecewise-linear envelope at `pos` (flat outside its nodes). */
export function envelopeAt(nodes: { pos: number; value: number }[], pos: number): number {
  if (nodes.length === 0) return 0;
  if (pos <= nodes[0].pos) return nodes[0].value;
  for (let i = 1; i < nodes.length; i++) {
    if (pos <= nodes[i].pos) {
      const a = nodes[i - 1];
      const b = nodes[i];
      return b.pos === a.pos ? b.value : a.value + ((b.value - a.value) * (pos - a.pos)) / (b.pos - a.pos);
    }
  }
  return nodes[nodes.length - 1].value;
}

/** Replaces one envelope of a clip. Tempo needs at least one node. */
export function setEnvelope(doc: MixDoc, clipId: string, target: EnvelopeTarget, nodes: EnvNode[] | TempoNode[] | { pos: number; cents: number; flags: number }[]): MixDoc {
  switch (target.kind) {
    case "gain":
      return replaceClip(doc, clipId, (c) => ({ ...c, gain: nodes as EnvNode[] }));
    case "tempo":
      if (nodes.length === 0) return doc;
      return replaceClip(doc, clipId, (c) => ({ ...c, tempo: nodes as TempoNode[] }));
    case "key":
      return replaceClip(doc, clipId, (c) => ({ ...c, key: nodes as Clip["key"] }));
    case "param":
      return updateProcess(doc, clipId, target.processId, (p) =>
        p.kind === "tool" ? { ...p, params: { ...p.params, [target.key]: nodes as EnvNode[] } } : p,
      );
  }
}

// ---------------------------------------------------------------------------------------------
// Transitions
// ---------------------------------------------------------------------------------------------

/** A linear crossfade between two clips over a timeline range. */
export function newTransition(from: string, to: string, start: number, length: number, id: string = newId()): TransitionDoc {
  return {
    id,
    name: "Transition",
    bypassed: false,
    beatMatch: false,
    crossfadeType: 1,
    curve: [
      { x: 0, value: 0 },
      { x: 1, value: 1 },
    ],
    between: { from, to },
    start,
    length,
    trimStart: 0,
    trimEnd: 0,
    limitSize: false,
    startSec: secOf(start),
    lengthSec: secOf(length),
  };
}

export function addTransition(doc: MixDoc, t: TransitionDoc): MixDoc {
  return refresh({ ...doc, transitions: [...doc.transitions, t] });
}

export function updateTransition(doc: MixDoc, id: string, fn: (t: TransitionDoc) => TransitionDoc): MixDoc {
  return refresh({ ...doc, transitions: doc.transitions.map((t) => (t.id === id ? fn(t) : t)) });
}

export function deleteTransitions(doc: MixDoc, ids: string[]): MixDoc {
  return refresh({ ...doc, transitions: doc.transitions.filter((t) => !ids.includes(t.id)) });
}

/** Sets the crossfade curve (points sorted by x, clamped to 0..1). */
export function setCurve(doc: MixDoc, id: string, curve: CurveNode[]): MixDoc {
  const c = curve
    .map((n) => ({ x: Math.min(1, Math.max(0, n.x)), value: Math.min(1, Math.max(0, n.value)) }))
    .sort((a, b) => a.x - b.x);
  return updateTransition(doc, id, (t) => ({ ...t, curve: c }));
}

/** Moves / resizes a transition on the timeline, snapped when asked. */
export function moveTransition(doc: MixDoc, id: string, start: number, length: number | null, opts?: SnapOptions): MixDoc {
  return updateTransition(doc, id, (t) => {
    const targets = opts && opts.mode !== "off" ? snapTargets(doc, opts.mode, [], [start - SR, start + SR], opts.sections) : [];
    const s = Math.max(0, opts ? snap(start, targets, opts.threshold) : start);
    return { ...t, start: s, length: Math.max(MIN_LENGTH, length ?? t.length) };
  });
}

// ---------------------------------------------------------------------------------------------
// Tempo changes
// ---------------------------------------------------------------------------------------------

/** Changes a clip's tempo curve and moves everything that lives on its clip-local axis (length,
 * trims, tools, loops, key and gain nodes) with the audio, so each stays on the same musical
 * position. The audible start stays where it is on the timeline. This is the remap
 * `local' = new.localAt(old.sourceAt(local))`; envelope nodes inside a tool scale with its length. */
export function retempoClip(c: Clip, tempo: TempoNode[]): Clip {
  const old = new TempoMap(c.tempo);
  const nw = new TempoMap(tempo);
  const remap = (p: number) => nw.localAt(old.sourceAt(p));
  const length = remap(c.length);
  const trimStart = remap(c.trimStart);
  const trimEnd = Math.max(0, length - remap(c.length - c.trimEnd));
  const processes = c.processes.map((p): ProcessDoc => {
    const start = remap(p.start);
    const len = remap(p.start + p.length) - start;
    const ratio = p.length > 0 ? len / p.length : 1;
    const moved = { ...p, start, length: len };
    if (moved.kind === "loop") return rescaleProcess({ ...moved, iterationLength: moved.iterationLength * ratio }, p.length);
    return rescaleProcess(moved, p.length);
  });
  const at = (n: { pos: number }) => Math.round(remap(n.pos));
  return {
    ...c,
    tempo,
    length,
    trimStart,
    trimEnd,
    start: c.start + c.trimStart - trimStart,
    processes,
    key: c.key.map((n) => ({ ...n, pos: at(n) })),
    gain: c.gain.map((n) => ({ ...n, pos: at(n) })),
    volume: { ...c.volume, length },
  };
}

/** Plays a clip at `bpm` (constant tempo curve), moving its contents with the audio. */
export function setClipBpm(doc: MixDoc, clipId: string, bpm: number): MixDoc {
  const c = clipOf(doc, clipId);
  if (!c || !(bpm > 0) || !(c.bpm > 0)) return doc;
  return replaceClip(doc, clipId, (x) => retempoClip(x, [{ pos: 0, speed: bpm / x.bpm }]));
}

/** Applies a `mixBeatmatch` suggestion. */
export function applyBeatmatch(doc: MixDoc, s: BeatmatchSuggestion): MixDoc {
  return replaceClip(doc, s.clipId, (c) => {
    const r = retempoClip(c, s.tempo);
    return { ...r, start: s.start, length: s.length, trimStart: s.trimStart, trimEnd: s.trimEnd, volume: { ...r.volume, length: s.length } };
  });
}

/** Applies a `mixAutoTransition` suggestion: the transition and its tools. With
 * `replaceConflicts` the EQ tools it overlaps are removed first; without, tools with conflicts
 * are skipped (saving would refuse the overlap). */
export function applyTransition(doc: MixDoc, s: TransitionSuggestion, replaceConflicts = false): MixDoc {
  let d = addTransition(doc, s.transition);
  for (const t of s.tools) {
    if (t.conflicts.length > 0) {
      if (!replaceConflicts) continue;
      d = deleteProcesses(d, t.conflicts);
    }
    d = addProcess(d, t.clipId, t.tool);
  }
  return d;
}

/** What `buildRecipe` (state/transitionRecipes.ts) needs from this file. */
export const recipeKit: Kit = { newTool, conflicts: toolConflicts };

// ---------------------------------------------------------------------------------------------
// Clipboard
// ---------------------------------------------------------------------------------------------

export type Clipboard =
  | { kind: "clips"; clips: Clip[] }
  | { kind: "processes"; processes: ProcessDoc[] };

/** Copies clips (with their contents). */
export function copyClips(doc: MixDoc, ids: string[]): Clipboard | null {
  const clips = sortedClips(doc).filter((c) => ids.includes(c.id));
  return clips.length ? { kind: "clips", clips } : null;
}

/** Copies processes of one clip. */
export function copyProcesses(doc: MixDoc, clipId: string, ids: string[]): Clipboard | null {
  const c = clipOf(doc, clipId);
  const processes = (c?.processes ?? []).filter((p) => ids.includes(p.id));
  return processes.length ? { kind: "processes", processes } : null;
}

function freshProcesses(ps: ProcessDoc[]): ProcessDoc[] {
  const map = new Map<string, string>();
  for (const p of ps) map.set(p.id, newId());
  return ps.map((p) => {
    const q: ProcessDoc = { ...p, id: map.get(p.id) as string };
    delete (q as { legacySessionId?: string }).legacySessionId;
    // a loop continues its copy of the parent when that was copied too, else it starts a chain
    if (q.kind === "loop") q.parent = q.parent && map.has(q.parent) ? (map.get(q.parent) as string) : null;
    return q;
  });
}

/** Pastes copied clips so that the earliest audible start lands on `at`, on free decks. Returns
 * the new document and the ids of the pasted clips. */
export function pasteClips(doc: MixDoc, cb: Clipboard & { kind: "clips" }, at: number): { doc: MixDoc; ids: string[] } {
  const origin = Math.min(...cb.clips.map((c) => c.visibleStart));
  let d = doc;
  const ids: string[] = [];
  for (const c of cb.clips) {
    const id = newId();
    const copy: Clip = { ...c, id, processes: freshProcesses(c.processes), track: c.track };
    delete (copy as { legacySessionId?: string }).legacySessionId;
    const vs = at + (c.visibleStart - origin);
    d = addClip(d, copy, vs + 0, undefined);
    // addClip placed the audible start at vs; keep the original channel when it is free
    const ve = vs + (c.visibleEnd - c.visibleStart);
    const free = freeChannel({ ...d, clips: d.clips.filter((x) => x.id !== id) }, vs, ve);
    d = replaceClip(d, id, (x) => ({ ...x, channel: free }));
    ids.push(id);
  }
  return { doc: d, ids };
}

/** Pastes copied processes into a clip so that the earliest starts at clip-local `atLocal`. */
export function pasteProcesses(doc: MixDoc, clipId: string, cb: Clipboard & { kind: "processes" }, atLocal: number): { doc: MixDoc; ids: string[] } {
  const origin = Math.min(...cb.processes.map((p) => p.start));
  const fresh = freshProcesses(cb.processes);
  let d = doc;
  fresh.forEach((p, i) => {
    d = addProcess(d, clipId, { ...p, start: atLocal + (cb.processes[i].start - origin) });
  });
  return { doc: d, ids: fresh.map((p) => p.id) };
}

// ---------------------------------------------------------------------------------------------
// Tool catalogue
// ---------------------------------------------------------------------------------------------

const prm = (name: string, key: string, label: string, min: number, max: number, unit: string, flat: number) => ({
  key: `${name}-${key}`,
  label,
  min,
  max,
  unit,
  flat,
});

/** Tools the editor can create always (Delay has no sample and is not offered; Trans is `TRANS_TYPE`,
 * offered by choice; Volume is deliberately never offered, see `TRANS_TYPE`).
 * `flat` is the neutral value a new tool starts at. Ranges are those of the Editor's demo mix; the
 * EQ semantics (Hi / Mid 0 = flat, Lo 0 = flat, -100 = kill) are a hypothesis from them. The
 * parameter keys are exactly the ones `pm_formats::mix::Effect::automation_keys` lists. */
export const TOOL_TYPES: ToolType[] = [
  { name: "Reverb", label: "Reverb", group: "fx", params: [prm("Reverb", "Time", "Time", 0, 50, "", 25), prm("Reverb", "Wet/Dry", "Wet/Dry", 0, 100, "%", 30)] },
  {
    name: "Echo",
    label: "Echo",
    group: "fx",
    params: [prm("Echo", "Time", "Time", 0, 3000, "ms", 1500), prm("Echo", "Time ", "Beat division", -3, 3, "", 0), prm("Echo", "Wet/Dry", "Wet/Dry", 0, 100, "%", 40)],
  },
  {
    name: "Roll",
    label: "Roll",
    group: "fx",
    params: [prm("Roll", "Time", "Time", 0, 3000, "ms", 1500), prm("Roll", "Time ", "Beat division", -3, 3, "", 0), prm("Roll", "Wet/Dry", "Wet/Dry", 0, 100, "%", 50)],
  },
  { name: "Wah", label: "Wah", group: "fx", params: [prm("Wah", "CutOff", "Cut-off", -100, 100, "", 0)] },
  { name: "Crush", label: "Crush", group: "fx", params: [prm("Crush", "Crush", "Crush", 0, 100, "", 30)] },
  { name: "FILTER", label: "Filter", group: "colour", params: [prm("FILTER", "CutOff", "Cut-off", -100, 100, "", 0)] },
  {
    name: "EQ",
    label: "EQ",
    group: "eq",
    // the Editor's key order (std::map: alphabetical)
    params: [prm("EQ", "Hi", "High", 0, 4, "", 0), prm("EQ", "Lo", "Low", -100, 0, "", 0), prm("EQ", "Mid", "Mid", 0, 4, "", 0)],
  },
];

/** The Trans tool (a tempo-synced gate: the audio chops on and off in eighth notes). The Editor's
 * tool menu has it, and the model, the file reader / writer and the audio engine all handle it, but
 * NO document with a Trans tool has ever been seen (notes/mix-format.md section 0 and 8): its class
 * version and tracking are assumed, and it is written with NO automation envelope because the
 * Editor's key names for it are unknown (the engine reads an optional `Trans-Wet/Dry` and plays at
 * full strength without it). So it is offered only while the "Trans tool" setting is on (default
 * on), the panel says it is unchecked on hardware, and `params` is empty.
 *
 * Volume is the opposite case and is NOT offered: the Editor has no Volume tool (its tool menu is
 * X-FADE, EQ, Colour FX, Beat FX, Loop, Reverse, Ignore), the per-track volume is the clip's gain
 * envelope (`Volume-Volume` of the track's volume shell, which the gain lane already edits and
 * which is the verified form), and a `VolumeAudioProcess` as an item of the tool lane appears in no
 * known document. A mix that already contains one still opens, plays and saves unchanged. */
export const TRANS_TYPE: ToolType = { name: "Trans", label: "Trans", group: "fx", params: [] };

/** The tools the add menu offers: the catalogue (as the card presents it), plus Trans when `trans`
 * is true. Looking up an existing tool by name uses `toolTypesFor(vocal)`, which is never wrong for
 * Trans (it has no parameters to look up). */
export function addableToolTypes(vocal: VocalMode, trans: boolean): ToolType[] {
  return [...toolTypesFor(vocal), ...(trans ? [TRANS_TYPE] : [])];
}

/** The catalogue as the card presents it. On a Pacemaker set up for the Vocal filter (Revival
 * firmware 180096073, `revival-vocal` on the card) the Wah tool is a vocal filter: it is called
 * "Vocal" and its one parameter says which way cuts. The effect name and the parameter key stay
 * `Wah` / `Wah-CutOff`, so documents and the device are unaffected. With the filter off (or the
 * kill switch on) this is `TOOL_TYPES` itself. Mirrors `toolLabel` in views/mixes/layout.ts. */
export function toolTypesFor(vocal: VocalMode): ToolType[] {
  if (vocal === "off") return TOOL_TYPES;
  const label = vocal === "vocal" ? "Vocals: cut (-) / keep (+)" : "Vocals: keep (-) / cut (+)";
  return TOOL_TYPES.map((t) => (t.name === "Wah" ? { ...t, label: "Vocal", params: t.params.map((p) => ({ ...p, label })) } : t));
}

/** Effect class (the tagged members of an `EffectDoc`) the Editor writes for each tool type. */
const EFFECT_CLASS: Record<string, Record<string, unknown>> = {
  Reverb: { class: "fx", active: true },
  Echo: { class: "delay", delayType: 1, active: true, syncAgainstBeat: true },
  Roll: { class: "delay", delayType: 2, active: true, syncAgainstBeat: true },
  Wah: { class: "peq" },
  Crush: { class: "crush" },
  FILTER: { class: "filter" },
  EQ: { class: "eq" },
};
