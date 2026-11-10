// The mix editor's document store: the open MixDoc, dirty flag, selection, undo / redo, clipboard,
// and the actions the timeline UI calls. All editing arithmetic (snapping, trims, tempo changes,
// paste, ...) is in the pure state/mixedit.ts; this file only adds state and the backend calls.
//
// Usage from the UI:
//   await mixes.open(id)                  // or mixes.create(name)
//   mixes.doc                             // the current MixDoc (never mutate it)
//   mixes.moveClip(id, newStart, { live: true })   // call on every pointer move; one undo step per drag
//   mixes.endGesture()                    // on pointer up: the next edit starts a new undo step
//   await mixes.save()                    // validates in the backend, writes, clears `dirty`
//
// `doc` is a raw (non-proxied) state: documents are replaced, never mutated, so snapshots for undo
// share the beat grids and untouched clips.
import {
  mixAutoTransition,
  mixBeatmatch,
  mixList,
  mixMakeClip,
  mixNew,
  mixOpen,
  mixSave,
  mixValidate,
  type BeatmatchSuggestion,
  type Clip,
  type EnvNode,
  type MixDoc,
  type MixSummary,
  type ProcessDoc,
  type TransitionSuggestion,
} from "$lib/api/mixes";
import { renderMixPreview } from "$lib/api/export";
import { session } from "./session.svelte";
import { toast } from "./toast.svelte";
import * as E from "./mixedit";

/** Undo steps kept. Snapshots share structure, so this is generous; the oldest go first. */
const MAX_UNDO = 2000;
/** Edits of one gesture closer together than this (ms) merge into one undo step. */
const COALESCE_MS = 1500;

type Snapshot = { doc: MixDoc; label: string };

export type EditOptions = {
  /** Edits with the same group within `COALESCE_MS` of each other are one undo step (a drag). */
  group?: string;
  /** False for changes that are not part of the saved document (the reference tempo). */
  dirties?: boolean;
};

class Mixes {
  /** The open document, or null. Replace it through the actions below, never mutate it. */
  doc = $state.raw<MixDoc | null>(null);
  /** Ids (clips, processes, transitions) that are selected. */
  selected = $state.raw<string[]>([]);
  /** Edits since the last open / save. */
  dirty = $state(false);
  undoDepth = $state(0);
  redoDepth = $state(0);
  /** Label of the edit undo would revert / redo would redo (for menu text). */
  undoLabel = $state("");
  redoLabel = $state("");
  /** Snap mode and distance in samples; the timeline view sets `threshold` from its zoom. */
  snap = $state<E.SnapMode>("bar");
  threshold = $state(E.SR / 20);
  /** What `mixValidate` says would stop a save, refreshed after edits. */
  problems = $state.raw<string[]>([]);
  busy = $state(false);
  /** Copied clips or processes. */
  clipboard = $state.raw<E.Clipboard | null>(null);
  /** The mix list, for pickers (refreshed by `refreshList`). */
  list = $state.raw<MixSummary[]>([]);

  #undo: Snapshot[] = [];
  #redo: Snapshot[] = [];
  #savedDoc: MixDoc | null = null;
  #group = "";
  #groupAt = 0;
  #checkTimer: ReturnType<typeof setTimeout> | undefined;

  get root(): string | null {
    return session.library?.root ?? null;
  }

  // ------------------------------------------------------------------------------------------
  // Document lifecycle
  // ------------------------------------------------------------------------------------------

  async refreshList() {
    if (this.root) this.list = await mixList(this.root);
  }

  /** Opens a mix from the library (the caller handles unsaved changes first: see `dirty`). */
  async open(id: number) {
    const root = this.root;
    if (!root) throw new Error("no library is open");
    this.busy = true;
    try {
      this.#load(await mixOpen(root, id));
    } finally {
      this.busy = false;
    }
  }

  /** Creates an empty mix and opens it. */
  async create(name: string) {
    const root = this.root;
    if (!root) throw new Error("no library is open");
    const id = await mixNew(root, name);
    await this.refreshList();
    await this.open(id);
    return id;
  }

  /** Writes the document. Throws the backend's message naming every problem when it is refused. */
  async save() {
    const root = this.root;
    const doc = this.doc;
    if (!root || !doc) return;
    this.busy = true;
    try {
      const saved = await mixSave(root, doc.id, doc);
      this.#savedDoc = doc;
      this.dirty = false;
      this.problems = [];
      if (doc.meta) {
        this.doc = { ...doc, name: saved.name, meta: { ...doc.meta, dateModified: saved.dateModified, playTimeSecs: saved.playTimeSecs } };
        this.#savedDoc = this.doc;
      }
      toast.show(`Saved "${saved.name}"`, "ok");
      void this.refreshList();
    } finally {
      this.busy = false;
    }
  }

  /** Throws away the changes and re-reads the mix from the library. */
  async revert() {
    if (this.doc) await this.open(this.doc.id);
  }

  close() {
    this.doc = null;
    this.selected = [];
    this.dirty = false;
    this.problems = [];
    this.#savedDoc = null;
    this.#undo = [];
    this.#redo = [];
    this.#group = "";
    this.#syncDepth();
  }

  #load(doc: MixDoc) {
    this.close();
    this.doc = doc;
    this.#savedDoc = doc;
  }

  // ------------------------------------------------------------------------------------------
  // Undo / redo
  // ------------------------------------------------------------------------------------------

  #syncDepth() {
    this.undoDepth = this.#undo.length;
    this.redoDepth = this.#redo.length;
    this.undoLabel = this.#undo.at(-1)?.label ?? "";
    this.redoLabel = this.#redo.at(-1)?.label ?? "";
  }

  /** Applies a change. `fn` gets the current document and returns the new one (use the pure
   * functions in mixedit.ts); returning the same object is a no-op. */
  edit(label: string, fn: (doc: MixDoc) => MixDoc, opts: EditOptions = {}) {
    const before = this.doc;
    if (!before) return;
    const after = fn(before);
    if (after === before) return;
    const now = Date.now();
    const merge = !!opts.group && opts.group === this.#group && now - this.#groupAt < COALESCE_MS && this.#undo.length > 0;
    if (!merge) {
      this.#undo.push({ doc: before, label });
      if (this.#undo.length > MAX_UNDO) this.#undo.shift();
    }
    this.#redo = [];
    this.#group = opts.group ?? "";
    this.#groupAt = now;
    this.doc = after;
    if (opts.dirties !== false) this.dirty = true;
    this.#syncDepth();
    this.#scheduleCheck();
  }

  /** Ends the current gesture: the next edit starts a new undo step even in the same group. */
  endGesture() {
    this.#group = "";
  }

  undo() {
    const s = this.#undo.pop();
    if (!s || !this.doc) return;
    this.#redo.push({ doc: this.doc, label: s.label });
    this.doc = s.doc;
    this.#afterHistory();
  }

  redo() {
    const s = this.#redo.pop();
    if (!s || !this.doc) return;
    this.#undo.push({ doc: this.doc, label: s.label });
    this.doc = s.doc;
    this.#afterHistory();
  }

  #afterHistory() {
    this.#group = "";
    this.dirty = this.doc !== this.#savedDoc;
    // drop selected ids that no longer exist
    const doc = this.doc;
    if (doc) this.selected = this.selected.filter((id) => E.find(doc, id));
    this.#syncDepth();
    this.#scheduleCheck();
  }

  #scheduleCheck() {
    clearTimeout(this.#checkTimer);
    this.#checkTimer = setTimeout(() => void this.check(), 400);
  }

  /** Asks the backend what would stop a save; fills `problems`. */
  async check() {
    const root = this.root;
    const doc = this.doc;
    if (!root || !doc) return [];
    try {
      const p = await mixValidate(root, doc);
      if (this.doc === doc) this.problems = p;
      return p;
    } catch {
      return this.problems;
    }
  }

  // ------------------------------------------------------------------------------------------
  // Selection
  // ------------------------------------------------------------------------------------------

  select(ids: string[], additive = false) {
    this.selected = additive ? [...new Set([...this.selected, ...ids])] : ids;
  }

  toggleSelected(id: string) {
    this.selected = this.selected.includes(id) ? this.selected.filter((x) => x !== id) : [...this.selected, id];
  }

  clearSelection() {
    this.selected = [];
  }

  isSelected(id: string) {
    return this.selected.includes(id);
  }

  selectAllClips() {
    this.selected = this.doc?.clips.map((c) => c.id) ?? [];
  }

  // ------------------------------------------------------------------------------------------
  // Editing actions (thin wrappers over mixedit.ts)
  // ------------------------------------------------------------------------------------------

  #snapOpts(exclude: string[] = []): E.SnapOptions {
    return { mode: this.snap, threshold: this.threshold, exclude };
  }

  setName(name: string) {
    this.edit("Rename mix", (d) => (d.name === name ? d : { ...d, name }));
  }

  /** The reference tempo (not saved). */
  setTempo(bpm: number | null) {
    this.edit("Mix tempo", (d) => (d.tempoBpm === bpm ? d : { ...d, tempoBpm: bpm }), { dirties: false });
  }

  /** Fetches a clip for a library track and adds it with its audible start at `at` (samples;
   * default: right after the last clip). Selects it. */
  async addTrack(trackId: number, at?: number, channel?: number) {
    const root = this.root;
    if (!root || !this.doc) return;
    const clip = await mixMakeClip(root, trackId, this.doc.tempoBpm);
    this.addClip(clip, at, channel);
  }

  addClip(clip: Clip, at?: number, channel?: number) {
    this.edit("Add track", (d) => E.addClip(d, clip, at ?? Math.max(0, d.length), channel));
    this.selected = [clip.id];
  }

  /** Drag a clip: `newStart` is the new `start` (not the audible start); snapped per `snap`. */
  moveClip(id: string, newStart: number, opts: { channel?: number; live?: boolean } = {}) {
    this.edit("Move clip", (d) => E.moveClip(d, id, newStart, { ...this.#snapOpts(), channel: opts.channel }), { group: `move:${id}` });
    if (!opts.live) this.endGesture();
  }

  /** Drag a clip edge to timeline position `pos`. */
  resizeClip(id: string, edge: "left" | "right", pos: number, opts: { live?: boolean } = {}) {
    this.edit("Trim clip", (d) => E.resizeClip(d, id, edge, pos, this.#snapOpts()), { group: `trim:${id}:${edge}` });
    if (!opts.live) this.endGesture();
  }

  setChannel(id: string, channel: number) {
    this.edit("Change deck", (d) => E.updateClip(d, id, (c) => ({ ...c, channel })));
  }

  addTool(clipId: string, type: string, localStart: number, length: number) {
    const tool = E.newTool(type, localStart, length);
    this.edit(`Add ${type}`, (d) => E.addProcess(d, clipId, tool));
    this.selected = [tool.id];
    return tool;
  }

  addRegion(clipId: string, kind: "loop" | "ignore" | "reverse", localStart: number, length: number) {
    const r = E.newRegion(kind, localStart, length);
    this.edit(`Add ${kind}`, (d) => E.addProcess(d, clipId, r));
    this.selected = [r.id];
    return r;
  }

  moveProcess(clipId: string, id: string, newLocalStart: number, opts: { live?: boolean } = {}) {
    this.edit("Move tool", (d) => E.moveProcess(d, clipId, id, newLocalStart, this.#snapOpts()), { group: `pmove:${id}` });
    if (!opts.live) this.endGesture();
  }

  resizeProcess(clipId: string, id: string, edge: "left" | "right", localPos: number, opts: { live?: boolean } = {}) {
    this.edit("Resize tool", (d) => E.resizeProcess(d, clipId, id, edge, localPos, this.#snapOpts()), { group: `presize:${id}:${edge}` });
    if (!opts.live) this.endGesture();
  }

  updateProcess(clipId: string, id: string, label: string, fn: (p: ProcessDoc) => ProcessDoc) {
    this.edit(label, (d) => E.updateProcess(d, clipId, id, fn));
  }

  /** Replaces one envelope (gain, tempo, key or a tool parameter), e.g. after dragging a node. */
  setEnvelope(clipId: string, target: E.EnvelopeTarget, nodes: Parameters<typeof E.setEnvelope>[3], opts: { live?: boolean } = {}) {
    this.edit("Edit envelope", (d) => E.setEnvelope(d, clipId, target, nodes), { group: `env:${clipId}:${JSON.stringify(target)}` });
    if (!opts.live) this.endGesture();
  }

  /** Plays a clip at `bpm`, moving its tools with the audio. */
  setClipBpm(clipId: string, bpm: number) {
    this.edit("Change tempo", (d) => E.setClipBpm(d, clipId, bpm));
  }

  addTransition(from: string, to: string, start: number, length: number) {
    const t = E.newTransition(from, to, start, length);
    this.edit("Add transition", (d) => E.addTransition(d, t));
    this.selected = [t.id];
    return t;
  }

  /** Moves / resizes the crossfade curve of a transition. */
  setCurve(id: string, curve: Parameters<typeof E.setCurve>[2], opts: { live?: boolean } = {}) {
    this.edit("Edit crossfade", (d) => E.setCurve(d, id, curve), { group: `curve:${id}` });
    if (!opts.live) this.endGesture();
  }

  /** Deletes what is selected (the UI confirms before calling this when it is more than a few
   * items, naming them: a destructive action must say what it removes). */
  deleteSelection() {
    const doc = this.doc;
    if (!doc || this.selected.length === 0) return;
    const ids = this.selected;
    const clips = ids.filter((id) => E.find(doc, id)?.kind === "clip");
    const processes = ids.filter((id) => E.find(doc, id)?.kind === "process");
    const transitions = ids.filter((id) => E.find(doc, id)?.kind === "transition");
    this.edit("Delete", (d) => {
      let x = d;
      if (processes.length) x = E.deleteProcesses(x, processes);
      if (transitions.length) x = E.deleteTransitions(x, transitions);
      if (clips.length) x = E.deleteClips(x, clips);
      return x;
    });
    this.selected = [];
  }

  // ------------------------------------------------------------------------------------------
  // Clipboard
  // ------------------------------------------------------------------------------------------

  /** Copies the selected clips, or the selected processes of one clip. */
  copy() {
    const doc = this.doc;
    if (!doc) return;
    const sel = this.selected.map((id) => E.find(doc, id)).filter((f) => f !== null);
    const clips = sel.filter((f) => f.kind === "clip").map((f) => f.clip.id);
    if (clips.length) {
      this.clipboard = E.copyClips(doc, clips);
      return;
    }
    const procs = sel.filter((f) => f.kind === "process");
    if (procs.length) {
      const clipId = procs[0].clip.id;
      this.clipboard = E.copyProcesses(doc, clipId, procs.filter((p) => p.clip.id === clipId).map((p) => p.process.id));
    }
  }

  cut() {
    this.copy();
    this.deleteSelection();
  }

  /** Pastes clips at timeline `at` (samples), or processes into `clipId` at clip-local `at`. */
  paste(at: number, clipId?: string) {
    const cb = this.clipboard;
    if (!cb || !this.doc) return;
    if (cb.kind === "clips") {
      let ids: string[] = [];
      this.edit("Paste clips", (d) => {
        const r = E.pasteClips(d, cb, at);
        ids = r.ids;
        return r.doc;
      });
      this.selected = ids;
    } else if (clipId) {
      let ids: string[] = [];
      this.edit("Paste tools", (d) => {
        const r = E.pasteProcesses(d, clipId, cb, at);
        ids = r.ids;
        return r.doc;
      });
      this.selected = ids;
    }
  }

  // ------------------------------------------------------------------------------------------
  // Suggestions
  // ------------------------------------------------------------------------------------------

  /** Asks the backend to beat-match a clip to the one before it, and applies the result as one
   * undo step. Returns the suggestion (its `notes` are for the user). Throws the backend's
   * explanation when there is nothing to match to or no beat grid. */
  async beatmatch(clipId: string): Promise<BeatmatchSuggestion> {
    const root = this.root;
    if (!root || !this.doc) throw new Error("no mix is open");
    const s = await mixBeatmatch(root, this.doc, clipId);
    this.edit("Beat-match", (d) => E.applyBeatmatch(d, s));
    return s;
  }

  /** Asks for a transition between two clips. Returns it without applying when
   * `apply` is false (to preview or let the user choose); apply later with `applyTransition`. */
  async autoTransition(fromClipId: string, toClipId: string, apply = true): Promise<TransitionSuggestion> {
    const root = this.root;
    if (!root || !this.doc) throw new Error("no mix is open");
    const s = await mixAutoTransition(root, this.doc, fromClipId, toClipId);
    if (apply) this.applyTransition(s);
    return s;
  }

  applyTransition(s: TransitionSuggestion, replaceConflicts = false) {
    this.edit("Auto transition", (d) => E.applyTransition(d, s, replaceConflicts));
    this.selected = [s.transition.id];
  }

  /** Renders `[fromSec, toSec]` of the open (possibly unsaved) document to a temporary file for
   * preview; play it with `playback`. The audio area implements `export_render_preview`. */
  async renderPreview(fromSec: number, toSec: number) {
    const root = this.root;
    if (!root || !this.doc) throw new Error("no mix is open");
    return renderMixPreview(root, this.doc, fromSec, toSec);
  }

  // ------------------------------------------------------------------------------------------
  // Helpers for views
  // ------------------------------------------------------------------------------------------

  /** Clips in timeline order. */
  get clips(): Clip[] {
    return this.doc ? E.sortedClips(this.doc) : [];
  }

  /** The envelope nodes a node-editing view works on, for convenience. */
  gainOf(clipId: string): EnvNode[] {
    return this.doc?.clips.find((c) => c.id === clipId)?.gain ?? [];
  }
}

export const mixes = new Mixes();
session.onReset(() => mixes.close());
