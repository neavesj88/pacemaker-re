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
import { vocalTools } from "$lib/views/mixes/vocalTools.svelte";
import {
  mixAutoTransition,
  mixBeatmatch,
  mixList,
  mixMakeClip,
  mixNew,
  mixOpen,
  mixSave,
  mixSaveAs,
  mixValidate,
  type BeatmatchSuggestion,
  type Clip,
  type EnvNode,
  type MixDoc,
  type MixSummary,
  type ProcessDoc,
  type TransitionSuggestion,
} from "$lib/api/mixes";
import { renderMixPreview, type PreviewGains } from "$lib/api/export";
import { tempoPrefs } from "$lib/views/mixes/tempoPrefs.svelte";
import { sections } from "./sections.svelte";
import { session } from "./session.svelte";
import { toast } from "./toast.svelte";
import * as E from "./mixedit";
import { chooseTempo, exceeds, stretchText, type TempoChoice } from "./tempoMatch";

/** Undo steps kept. Snapshots share structure, so this is generous; the oldest go first. */
const MAX_UNDO = 2000;
/** Edits of one gesture closer together than this (ms) merge into one undo step. */
const COALESCE_MS = 1500;

type Snapshot = { doc: MixDoc; label: string };

/** True when two documents hold the same saved content. Documents are replaced, never mutated,
 * so untouched parts are shared and a reference comparison per field is enough. The reference
 * tempo is not part of the saved mix, so it is ignored. */
function sameContent(a: MixDoc, b: MixDoc): boolean {
  if (a === b) return true;
  const x = a as Record<string, unknown>;
  const y = b as Record<string, unknown>;
  const keys = new Set([...Object.keys(x), ...Object.keys(y)]);
  keys.delete("tempoBpm");
  for (const k of keys) if (x[k] !== y[k]) return false;
  return true;
}

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
  /** The library changed this mix behind the editor while it holds unsaved edits (a restore, or a Sync that
   * replaced it): "changed" = another version is there now, "gone" = the mix is not in the library any more.
   * Saving is refused until the user chooses (reload, or keep the edits on purpose). */
  libraryChange = $state<"changed" | "gone" | null>(null);
  /** The library's stamp behind `libraryChange`, and the stamp the user already chose to overwrite ("Keep my
   * edits"), so the next reload with the same stamp does not ask the same question again. */
  libraryStamp: number | null = null;
  keptStamp: number | null = null;
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

  /** Re-reads the mix list, and hands it to the open library too: the sidebar count and the
   *  Libraries view's left pane read `session.library.mixes`, so a mix made, saved, renamed or
   *  deleted here would otherwise be missing there until the library was opened again. */
  async refreshList() {
    const root = this.root;
    if (!root) return;
    const list = await mixList(root);
    // a different library may have been opened while the list was being read
    if (this.root !== root) return;
    this.list = list;
    session.setMixes(list.map((m) => ({ ...m })));
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
    if (this.libraryChange === "gone") throw new Error("this mix is no longer in the library (a restore or sync removed it). Use Save as to keep your edits as a new mix.");
    if (this.libraryChange === "changed") throw new Error("this mix was changed in the library (a restore or sync) after you opened it, and saving would overwrite that. Reload it, or choose Keep my edits first.");
    this.busy = true;
    try {
      const saved = await mixSave(root, doc.id, doc);
      const stamp = (d: MixDoc): MixDoc =>
        d.meta ? { ...d, meta: { ...d.meta, dateModified: saved.dateModified, playTimeSecs: saved.playTimeSecs } } : d;
      // The user may have kept editing while the write was in flight: what was written is `doc`,
      // so it becomes the saved baseline, and the live document (possibly newer) stays dirty.
      this.#savedDoc = stamp(doc);
      const cur = this.doc;
      if (cur && cur.id === doc.id) {
        if (cur === doc) this.doc = this.#savedDoc;
        else this.doc = { ...stamp(cur), name: cur.name === doc.name ? saved.name : cur.name };
        this.dirty = !sameContent(this.doc, this.#savedDoc);
        this.problems = this.dirty ? this.problems : [];
        if (this.dirty) this.#scheduleCheck();
      }
      toast.show(`Saved "${saved.name}"`, "ok");
      void this.refreshList();
    } finally {
      this.busy = false;
    }
  }

  /** Writes the open document (with its unsaved edits) as a NEW mix called `name` and opens that
   * mix in its place: the undo history is cleared, because its snapshots belong to the old mix id
   * and undoing into one would save over the original. The mix it came from keeps its last saved
   * content. Throws the backend's message (taken name, blocking problems) and changes nothing. */
  async saveAs(name: string) {
    const root = this.root;
    const doc = this.doc;
    if (!root || !doc) throw new Error("no mix is open");
    this.busy = true;
    try {
      const saved = await mixSaveAs(root, doc, name);
      vocalTools.copy(doc.id, saved.id);
      void this.refreshList();
      try {
        this.#load(await mixOpen(root, saved.id));
      } catch (e) {
        // the copy exists; do not let a retry in the dialog run into its own name
        toast.show(`Saved as "${saved.name}", but it could not be opened: ${e instanceof Error ? e.message : String(e)}`, "bad", 12000);
      }
      return saved;
    } finally {
      this.busy = false;
    }
  }

  /** Throws away the changes and re-reads the mix from the library. */
  async revert() {
    if (this.doc) await this.open(this.doc.id);
  }

  /** The user chose to keep the edits over the version the library has now: the next Save overwrites it. */
  keepEdits() {
    if (this.libraryChange === "changed") {
      this.keptStamp = this.libraryStamp;
      this.libraryChange = null;
    }
  }

  close() {
    this.libraryChange = null;
    this.libraryStamp = null;
    this.keptStamp = null;
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
    let after = fn(before);
    if (after === before) return;
    after = E.settleDecks(before, after);
    // a mix that has just become empty forgets its reference tempo: the next first track sets it again
    if (before.clips.length > 0 && after.clips.length === 0) after = { ...after, tempoBpm: E.carryTempo(before.tempoBpm, after) };
    const now = Date.now();
    if (opts.dirties === false) {
      // not part of the saved document (the reference tempo): no undo step, history untouched
      this.doc = after;
      return;
    }
    const merge = !!opts.group && opts.group === this.#group && now - this.#groupAt < COALESCE_MS && this.#undo.length > 0;
    if (!merge) {
      this.#undo.push({ doc: before, label });
      if (this.#undo.length > MAX_UNDO) this.#undo.shift();
    }
    this.#redo = [];
    this.#group = opts.group ?? "";
    this.#groupAt = now;
    this.doc = after;
    this.dirty = true;
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
    this.doc = { ...s.doc, tempoBpm: E.carryTempo(this.doc.tempoBpm, s.doc) };
    this.#afterHistory();
  }

  redo() {
    const s = this.#redo.pop();
    if (!s || !this.doc) return;
    this.#undo.push({ doc: this.doc, label: s.label });
    this.doc = { ...s.doc, tempoBpm: E.carryTempo(this.doc.tempoBpm, s.doc) };
    this.#afterHistory();
  }

  #afterHistory() {
    this.#group = "";
    this.dirty = !(this.doc && this.#savedDoc && sameContent(this.doc, this.#savedDoc));
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
    } catch (e) {
      // validation did not run: say so rather than showing a stale (maybe empty) list
      const p = [`validation could not run: ${e instanceof Error ? e.message : String(e)}`];
      if (this.doc === doc) this.problems = p;
      return p;
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

  /** Detected sections of a track (app-side estimates), for the "section" snap mode and the
   * auto-transition option; null for a track without any. */
  readonly sectionLookup: E.SectionsLookup = (trackId) => sections.of(trackId);

  #snapOpts(exclude: string[] = []): E.SnapOptions {
    return { mode: this.snap, threshold: this.threshold, exclude, sections: this.sectionLookup };
  }

  setName(name: string) {
    this.edit("Rename mix", (d) => (d.name === name ? d : { ...d, name }));
  }

  /** The reference tempo (not saved). */
  setTempo(bpm: number | null) {
    this.edit("Mix tempo", (d) => (d.tempoBpm === bpm ? d : { ...d, tempoBpm: bpm }), { dirties: false });
  }

  /** Fetches a clip for a library track and adds it with its audible start at `at` (samples;
   * default: right after the last clip). Selects it. With a mix tempo and the setting "match to
   * the mix tempo when added" on, the clip plays at the mix tempo or at double or half of it,
   * whichever stretches the track least; when even that is more than the setting's stretch limit
   * the clip keeps its own tempo (the other choice of that setting matches it anyway). */
  async addTrack(trackId: number, at?: number, channel?: number): Promise<{ choice: TempoChoice | null; matched: boolean }> {
    const root = this.root;
    if (!root || !this.doc) return { choice: null, matched: false };
    let clip = await mixMakeClip(root, trackId, null);
    const target = this.doc.tempoBpm;
    const prefs = tempoPrefs.value;
    const choice = target && prefs.matchOnAdd ? chooseTempo(clip.bpm, target) : null;
    const matched = !!choice && (!exceeds(choice.speed, prefs.warnPct) || prefs.overOnAdd === "match");
    if (choice && matched) clip = E.retempoClip(clip, [{ pos: 0, speed: choice.speed }]);
    this.addClip(clip, at, channel);
    return { choice, matched };
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

  /** `startBpm` is for Trans only (the tempo its gate follows). `label` is the name the Undo button shows
   * ("Vocal" for a Wah tool the user made as a Vocal tool). */
  addTool(clipId: string, type: string, localStart: number, length: number, startBpm = 0, label = type) {
    const tool = E.newTool(type, localStart, length, undefined, startBpm);
    this.edit(`Add ${label}`, (d) => E.addProcess(d, clipId, tool));
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
    const c = E.clipOf(this.doc, clipId);
    const hd = c ? E.matchHalfDouble(this.doc, clipId, s.targetBpm, tempoPrefs.value.warnPct) : null;
    if (hd) {
      // the backend lines the beats up at the reference tempo itself; this clip is closer to half
      // or double of it, so it plays at that instead and keeps its place (the beats are not
      // re-aligned: that needs the grids matched at the other ratio). Held back when it would
      // stretch the track more than the setting allows: never silently stretch a lot. A tempo
      // ramp keeps its shape (the whole curve is scaled).
      const choice = hd.choice;
      if (hd.outOfRange) {
        throw new Error(`nothing changed: ${choice.factor === 2 ? "double" : "half"} time would push part of this clip's tempo curve outside the 0.1 to 4 speed range`);
      }
      if (hd.held) {
        throw new Error(`nothing changed: ${choice.factor === 2 ? "double" : "half"} time would stretch this track ${stretchText(choice.stretchPct)}, more than the ${tempoPrefs.value.warnPct} % limit in the settings. Use "Sync to previous clip" and then "Match anyway" to do it regardless`);
      }
      this.edit("Match tempo", () => hd.doc);
      return { ...s, targetBpm: choice.playsAt, speed: choice.speed, notes: [...s.notes, `Matched at ${choice.factor === 2 ? "double" : "half"} time (${stretchText(choice.stretchPct)}); the beats were not re-aligned`] };
    }
    if (c && s.speed > 0 && exceeds(s.speed, tempoPrefs.value.warnPct)) {
      throw new Error(`nothing changed: matching the beats would stretch this track ${stretchText((s.speed - 1) * 100)}, more than the ${tempoPrefs.value.warnPct} % limit in the settings. Use "Sync to previous clip" and then "Match anyway" to do it regardless`);
    }
    this.edit("Beat-match", (d) => E.applyBeatmatch(d, s));
    return s;
  }

  /** Plays a clip at `target` BPM (or double / half of it, whichever stretches it least). A match
   * that stretches more than the setting allows is not applied unless `force`: the result says so,
   * so the caller can show the choice. */
  syncClip(clipId: string, target: number, force = false): E.SyncResult {
    const r = E.syncClipTo(this.doc!, clipId, target, tempoPrefs.value.warnPct, force);
    if (r.applied) this.edit("Match tempo", () => r.doc);
    return r;
  }

  /** Plays every clip at `target` BPM (or double / half of it), one undo step. Clips that would be
   * stretched more than the setting allows are left as they are and returned, so the caller can
   * name them. */
  syncAll(target: number): { applied: number; already: number; kept: { clip: Clip; choice: TempoChoice }[]; ramps: Clip[] } {
    const start = this.doc;
    if (!start) return { applied: 0, already: 0, kept: [], ramps: [] };
    let d = start;
    let applied = 0;
    let already = 0;
    const kept: { clip: Clip; choice: TempoChoice }[] = [];
    const ramps: Clip[] = [];
    for (const c of start.clips) {
      // a clip the user ramped is theirs: not flattened or rescaled by a bulk action (the single
      // clip "Sync" does scale a ramp, because there the user picked that clip)
      if (c.tempo.length > 1) {
        ramps.push(c);
        continue;
      }
      const r = E.syncClipTo(d, c.id, target, tempoPrefs.value.warnPct);
      if (r.applied) {
        d = r.doc;
        applied++;
      } else if (r.already) already++;
      else if (r.choice && r.choice.stretchPct !== 0 && exceeds(r.choice.speed, tempoPrefs.value.warnPct)) kept.push({ clip: c, choice: r.choice });
    }
    if (d !== start) this.edit("Match all to the mix tempo", () => d);
    return { applied, already, kept, ramps };
  }

  /** Asks for a transition between two clips, `bars` long (null = the backend's default of 16).
   * Returns it without applying when `apply` is false (to reshape it with a recipe or let the
   * user choose); apply later with `applyTransition`. */
  async autoTransition(fromClipId: string, toClipId: string, apply = true, bars: number | null = null): Promise<TransitionSuggestion> {
    const root = this.root;
    if (!root || !this.doc) throw new Error("no mix is open");
    const s = await mixAutoTransition(root, this.doc, fromClipId, toClipId, bars);
    if (apply) this.applyTransition(s);
    return s;
  }

  /** Adds a suggestion as one undo step. `replaceConflicts` removes the tools it overlaps (else
   * the new tool is skipped); `replacing` deletes that transition first (rebuilding it) and
   * `remove` the tools with those ids. */
  applyTransition(s: TransitionSuggestion, replaceConflicts = false, opts: { label?: string; replacing?: string; remove?: string[] } = {}) {
    this.edit(opts.label ?? "Auto transition", (d) => {
      let x = opts.replacing ? E.deleteTransitions(d, [opts.replacing]) : d;
      if (opts.remove?.length) x = E.deleteProcesses(x, opts.remove);
      return E.applyTransition(x, s, replaceConflicts);
    });
    this.selected = [s.transition.id];
  }

  /** Renders `[fromSec, toSec]` of the open (possibly unsaved) document to a temporary file for
   * preview; play it with `playback`. The audio area implements `export_render_preview`.
   * `gains` are the loudness gains an export would apply; `edgeFadeMs` fades the window's cuts (a
   * range preview). `vocalToolIds`: the Wah tools the app marks as Vocal tools, so the part is heard as the live player plays it. */
  async renderPreview(fromSec: number, toSec: number, gains?: PreviewGains, edgeFadeMs?: number, vocalToolIds?: string[]) {
    const root = this.root;
    if (!root || !this.doc) throw new Error("no mix is open");
    return renderMixPreview(root, this.doc, fromSec, toSec, gains, edgeFadeMs, vocalToolIds);
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
