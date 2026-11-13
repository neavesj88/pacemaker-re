// The set builder's working state: where the tracks came from, the set in play order, the tracks
// left out, the ones locked to a position, and a short undo history. The ordering itself is the
// pure solver (solver.ts); this file only holds state and calls it.
//
// Nothing here writes to the device. Saving the set as a case and creating a mix are separate
// actions the view asks for (cases.create, createMix.ts).
import { caseTracks } from "$lib/api/cases";
import type { Track } from "$lib/api/types";
import { selection } from "$lib/state/selection.svelte";
import { session } from "$lib/state/session.svelte";
import { parseKey } from "$lib/views/tracks/trackLogic";
import { energyOf } from "./energy";
import { setPrefs } from "./prefs.svelte";
import { bestInsert, evaluate, moveBlock, pinPositions, solve, solverOptions, type SetSummary, type SetTrack } from "./solver";

export type Source =
  | { kind: "case"; id: number; name: string }
  /** A fixed list of tracks: the Tracks view's selection when loaded, or ids passed in by a link. */
  | { kind: "tracks"; name: string; ids: number[] };

/** Why a track is not in the set. */
export type OutWhy = "you" | "length";
export type Out = { id: number; why: OutWhy };

type Snapshot = { order: number[]; out: Out[]; pinned: number[]; short: boolean; label: string };

const MAX_HISTORY = 50;

/** A library track as the solver sees it. A track that needs analysis has no trustworthy tempo. */
export function toSetTrack(t: Track, needsAnalysis: boolean): SetTrack {
  const key = parseKey(t.key);
  return {
    id: t.track_id,
    bpm: needsAnalysis || !(t.bpm > 0) ? 0 : t.bpm,
    key: key ? { num: key.num, letter: key.letter } : null,
    secs: t.play_time_secs,
    energy: energyOf(t.track_id),
  };
}

class SetBuilder {
  source = $state.raw<Source | null>(null);
  /** The set, in play order. */
  order = $state.raw<number[]>([]);
  /** Tracks not in the set: left out by the user, or dropped to reach the target length. */
  out = $state.raw<Out[]>([]);
  /** Tracks locked to the position they have now. */
  pinned = $state.raw<ReadonlySet<number>>(new Set());
  /** The pool is shorter than the target length, so every track is used. */
  short = $state(false);
  /** Entries of the source that are no longer in the library. */
  gone = $state.raw<number[]>([]);
  loading = $state(false);
  /** Settings changed or the order was edited by hand since the last solve. */
  edited = $state(false);
  canUndo = $state(false);
  undoLabel = $state("");

  /** The source's own order, to go back to. */
  #original: number[] = [];
  #history: Snapshot[] = [];

  /** Solver input for every track in the set or left out. */
  byId = $derived.by(() => {
    const lib = session.library;
    const m = new Map<number, SetTrack>();
    if (!lib) return m;
    const want = new Set([...this.order, ...this.out.map((o) => o.id)]);
    for (const t of lib.tracks) if (want.has(t.track_id)) m.set(t.track_id, toSetTrack(t, session.needs.has(t.track_id)));
    return m;
  });

  /** Library rows (title, artist) of the tracks in play, by id. */
  rows = $derived.by(() => {
    const m = new Map<number, Track>();
    const lib = session.library;
    if (!lib) return m;
    const want = new Set([...this.order, ...this.out.map((o) => o.id)]);
    for (const t of lib.tracks) if (want.has(t.track_id)) m.set(t.track_id, t);
    return m;
  });

  /** The set scored with the current settings: always live, so the strip is never stale. */
  summary = $derived<SetSummary>(evaluate(this.byId, this.order.filter((id) => this.byId.has(id)), solverOptions(setPrefs.value)));

  /** Does the set hold ids the library no longer has? */
  missing = $derived(this.order.filter((id) => !this.byId.has(id)));

  get ready(): boolean {
    return this.source !== null;
  }

  // ---- loading -----------------------------------------------------------------------------

  /** Reads a source and (by default setting) orders it. */
  async load(source: Source) {
    const lib = session.library;
    if (!lib) return;
    this.loading = true;
    try {
      const ids = source.kind === "case" ? await caseTracks(lib.root, source.id) : source.ids;
      if (session.library?.root !== lib.root) return;
      const have = new Set(lib.tracks.map((t) => t.track_id));
      const unique = [...new Set(ids)];
      this.source = source;
      this.gone = unique.filter((id) => !have.has(id));
      this.#original = unique.filter((id) => have.has(id));
      this.order = [...this.#original];
      this.out = [];
      this.pinned = new Set();
      this.short = false;
      this.edited = false;
      this.#history = [];
      this.#syncHistory();
      if (setPrefs.value.solveOnLoad && this.order.length > 1) this.solve(false);
    } catch (e) {
      session.error = String(e);
    } finally {
      this.loading = false;
    }
  }

  /** Reads the current source again (a fixed list of tracks keeps its own ids, not the current selection). */
  reload() {
    return this.source ? this.load(this.source) : Promise.resolve();
  }

  /** Is there work here that loading a source would throw away: locks, tracks left out by hand, or edits that can be undone? */
  get hasHandWork(): boolean {
    return this.source !== null && (this.canUndo || this.pinned.size > 0 || this.out.some((x) => x.why === "you"));
  }

  /** Words for what a load would lose, one line each (for the confirmation). */
  lostByLoading(): string[] {
    const lines: string[] = [];
    if (this.pinned.size > 0) lines.push(`${this.pinned.size} locked track${this.pinned.size === 1 ? "" : "s"} would be unlocked`);
    const you = this.out.filter((x) => x.why === "you").length;
    if (you > 0) lines.push(`${you} track${you === 1 ? "" : "s"} you left out would come back`);
    if (this.canUndo) lines.push("your edits to the order, and the undo history, would be dropped");
    return lines;
  }

  /** Loads the Tracks / Cases view's current selection. */
  loadSelection() {
    return this.load({ kind: "tracks", name: "Selected tracks", ids: [...selection.trackIds] });
  }

  clear() {
    this.source = null;
    this.order = [];
    this.out = [];
    this.pinned = new Set();
    this.gone = [];
    this.short = false;
    this.edited = false;
    this.#original = [];
    this.#history = [];
    this.#syncHistory();
  }

  // ---- history -----------------------------------------------------------------------------

  #remember(label: string) {
    this.#history.push({ order: this.order, out: this.out, pinned: [...this.pinned], short: this.short, label });
    if (this.#history.length > MAX_HISTORY) this.#history.shift();
    this.#syncHistory();
  }

  #syncHistory() {
    this.canUndo = this.#history.length > 0;
    this.undoLabel = this.#history.at(-1)?.label ?? "";
  }

  undo() {
    const s = this.#history.pop();
    if (!s) return;
    this.order = s.order;
    this.out = s.out;
    this.pinned = new Set(s.pinned);
    this.short = s.short;
    this.edited = true;
    this.#syncHistory();
  }

  // ---- solving -----------------------------------------------------------------------------

  /** Orders the set again. Locked tracks keep their place; tracks the user left out stay out;
   * tracks dropped for length are considered again. */
  solve(remember = true) {
    const o = solverOptions(setPrefs.value);
    const userOut = this.out.filter((x) => x.why === "you");
    const pool = [...this.order, ...this.out.filter((x) => x.why === "length").map((x) => x.id)]
      .map((id) => this.byId.get(id))
      .filter((t): t is SetTrack => !!t);
    if (remember) this.#remember("Re-solve");
    // positions are counted over the tracks the solver is given (ids the library lost are not slots)
    const pins = pinPositions(this.order, this.pinned, (id) => this.byId.has(id));
    const r = solve(pool, pins, o);
    this.order = r.order;
    this.out = [...userOut, ...r.dropped.map((id): Out => ({ id, why: "length" }))];
    this.short = r.short;
    this.pinned = new Set(r.order.filter((id) => this.pinned.has(id)));
    this.edited = false;
  }

  /** The settings changed: the set on screen is no longer what a solve would give. */
  settingsChanged() {
    if (this.source) this.edited = true;
  }

  /** Back to the order the tracks came in. Left-out tracks come back too. */
  restoreSourceOrder() {
    if (!this.source) return;
    this.#remember("Original order");
    this.order = this.#original.filter((id) => this.byId.has(id));
    this.out = [];
    this.pinned = new Set();
    this.short = false;
    this.edited = true;
  }

  // ---- editing by hand ---------------------------------------------------------------------

  /** Moves tracks so they sit before position `at` of the current order. Moved tracks are locked
   * where they land when the setting is on. */
  move(ids: number[], at: number) {
    const next = moveBlock(this.order, new Set(ids), at);
    if (next === this.order) return;
    this.#remember("Move");
    this.order = next;
    if (setPrefs.value.lockOnDrag) this.pinned = new Set([...this.pinned, ...ids]);
    this.edited = true;
  }

  /** One place up or down for the tracks (they keep their order among themselves). */
  nudge(ids: number[], dir: -1 | 1) {
    const idx = this.order.map((id, i) => (ids.includes(id) ? i : -1)).filter((i) => i >= 0);
    if (idx.length === 0) return;
    this.move(ids, dir < 0 ? Math.max(0, Math.min(...idx) - 1) : Math.max(...idx) + 2);
  }

  toggleLock(id: number) {
    const s = new Set(this.pinned);
    if (s.has(id)) s.delete(id);
    else if (this.order.includes(id)) s.add(id);
    this.pinned = s;
  }

  lockAll(on: boolean) {
    this.pinned = on ? new Set(this.order) : new Set();
  }

  /** Takes tracks out of the set (they stay in the library and in the source). */
  leaveOut(ids: number[]) {
    const gone = new Set(ids);
    if (!this.order.some((id) => gone.has(id))) return;
    this.#remember("Leave out");
    this.order = this.order.filter((id) => !gone.has(id));
    this.out = [...this.out, ...ids.filter((id) => !this.out.some((o) => o.id === id)).map((id): Out => ({ id, why: "you" }))];
    this.pinned = new Set([...this.pinned].filter((id) => !gone.has(id)));
    this.edited = true;
  }

  /** Puts a left-out track back where it costs the set least. */
  addBack(id: number) {
    const t = this.byId.get(id);
    if (!t || !this.out.some((o) => o.id === id)) return;
    this.#remember("Add back");
    const at = bestInsert(this.byId, this.order.filter((x) => this.byId.has(x)), t, solverOptions(setPrefs.value));
    this.order = [...this.order.slice(0, at), id, ...this.order.slice(at)];
    this.out = this.out.filter((o) => o.id !== id);
    this.edited = true;
  }

  /** Forgets entries the library no longer has. */
  dropMissing() {
    const m = new Set(this.missing);
    if (m.size === 0) return;
    this.#remember("Drop missing");
    this.order = this.order.filter((id) => !m.has(id));
    this.gone = [...this.gone, ...m];
  }
}

export const setBuilder = new SetBuilder();
session.onReset(() => setBuilder.clear());
