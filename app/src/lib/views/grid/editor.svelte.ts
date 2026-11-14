// The beat grid editor's working state: the open track, its grid and cue as edited so far, and
// an undo history for the session. Nothing reaches the device until `save()`. Edits go through
// the backend (`grid_edit`) so the math lives in one place (crates/pm-formats/src/ops_grid.rs).
import { peaks as fetchPeaks, type Peak } from "$lib/api/analysis";
import {
  gridAnalyse,
  gridEdit,
  gridLoad,
  gridSave,
  gridVtempoEdit,
  type GridDoc,
  type GridEdit,
  type GridLoad,
  type TempoEdit,
} from "$lib/api/grid";
import { session } from "$lib/state/session.svelte";
import { bpmRange } from "$lib/state/bpmRange.svelte";
import { jobs } from "$lib/state/jobs.svelte";
import { toast } from "$lib/state/toast.svelte";

/** Waveform columns requested for the whole track. */
export const PEAK_COLUMNS = 8192;

type Snap = { doc: GridDoc | null; cue: number | null; splits: number[] };

/** Median of the local tempi: the track tempo. */
export function trackBpm(doc: GridDoc | null): number {
  if (!doc || doc.beats.length === 0) return 0;
  const v = doc.beats.map((b) => b.bpm).sort((a, b) => a - b);
  return v[Math.floor(v.length / 2)];
}

/** "constant" or "N tempo segments" (runs of equal tempo to 0.05 BPM). */
export function tempoMap(doc: GridDoc | null): string {
  if (!doc || doc.beats.length === 0) return "none";
  let segments = 1;
  for (let i = 1; i < doc.beats.length; i++) if (Math.abs(doc.beats[i].bpm - doc.beats[i - 1].bpm) > 0.05) segments++;
  return segments === 1 ? "constant (1 segment)" : `variable (${segments} segments)`;
}

class GridEditor {
  loaded = $state<GridLoad | null>(null);
  /** Working grid. `$state.raw`: snapshots are compared by identity to know what is unsaved. */
  doc = $state.raw<GridDoc | null>(null);
  /** Working device cue in source frames, null = unset. */
  cue = $state<number | null>(null);
  peaks = $state.raw<Peak[] | null>(null);
  busy = $state(false);
  error = $state("");
  /** Waveform state, kept apart from `error` so a later edit does not wipe it. */
  peaksLoading = $state(false);
  peaksError = $state("");
  /** Frames where the user cut the grid into tempo sections. Part of the undo history, never
   *  written to the device (a cut with no tempo change leaves no trace in the .str). */
  splits = $state.raw<number[]>([]);

  #past = $state.raw<Snap[]>([]);
  #future = $state.raw<Snap[]>([]);
  // the saved state, to tell what is unsaved
  #baseDocState = $state.raw<GridDoc | null>(null);
  #baseCueState = $state<number | null>(null);
  /** Bumped on every open so a slow load of a track the user left cannot overwrite the new one. */
  #epoch = 0;

  trackId = $derived(this.loaded?.track_id ?? null);
  sampleRate = $derived(this.loaded?.sample_rate ?? 44100);
  duration = $derived(this.loaded?.duration_secs ?? 0);
  gridDirty = $derived(this.doc !== this.#baseDocState);
  cueDirty = $derived(this.cue !== this.#baseCueState);
  dirty = $derived(this.gridDirty || this.cueDirty);
  canUndo = $derived(this.#past.length > 0);
  canRedo = $derived(this.#future.length > 0);
  bpm = $derived(trackBpm(this.doc));

  secs(frames: number) {
    return frames / this.sampleRate;
  }

  frames(secs: number) {
    return Math.round(secs * this.sampleRate);
  }

  /** Shift of the first beat against the saved grid, in ms. */
  get offsetMs() {
    const a = this.doc?.beats[0]?.pos;
    const b = this.#baseDocState?.beats[0]?.pos;
    return a === undefined || b === undefined ? 0 : ((a - b) / this.sampleRate) * 1000;
  }

  clear() {
    this.#epoch++;
    this.loaded = null;
    this.doc = null;
    this.cue = null;
    this.peaks = null;
    this.peaksLoading = false;
    this.peaksError = "";
    this.error = "";
    this.splits = [];
    this.#past = [];
    this.#future = [];
    this.#baseDocState = null;
    this.#baseCueState = null;
  }

  async open(trackId: number) {
    const lib = session.library;
    if (!lib) return;
    const epoch = ++this.#epoch;
    this.busy = true;
    this.error = "";
    try {
      const l = await gridLoad(lib.root, trackId);
      if (epoch !== this.#epoch) return;
      this.loaded = l;
      this.doc = l.doc;
      this.cue = l.cue_frames;
      this.#baseDocState = l.doc;
      this.#baseCueState = l.cue_frames;
      this.splits = [];
      this.#past = [];
      this.#future = [];
      this.peaks = null;
      void this.loadPeaks();
    } catch (e) {
      if (epoch === this.#epoch) {
        this.loaded = null;
        this.doc = null;
        this.error = String(e);
      }
    } finally {
      if (epoch === this.#epoch) this.busy = false;
    }
  }

  /** Fetches the waveform of the open track; failures land in `peaksError` until retried. */
  async loadPeaks() {
    const lib = session.library;
    const l = this.loaded;
    if (!lib || !l) return;
    const epoch = this.#epoch;
    this.peaksLoading = true;
    this.peaksError = "";
    try {
      const p = await fetchPeaks(lib.root, l.location, PEAK_COLUMNS);
      if (epoch === this.#epoch) this.peaks = p;
    } catch (e) {
      if (epoch === this.#epoch) this.peaksError = String(e);
    } finally {
      if (epoch === this.#epoch) this.peaksLoading = false;
    }
  }

  #push() {
    this.#past = [...this.#past, { doc: this.doc, cue: this.cue, splits: this.splits }];
    this.#future = [];
  }

  /** Applies one edit; failures (for example a tempo out of range) are reported, not thrown. */
  async edit(e: GridEdit): Promise<boolean> {
    const doc = this.doc;
    const l = this.loaded;
    if (!doc || !l || this.busy) return false;
    this.busy = true;
    try {
      const next = await gridEdit(doc, e, l.sample_rate, l.total_frames > 0 ? l.total_frames : null);
      if (this.doc !== doc || this.loaded !== l) return false; // the track changed under us
      this.#push();
      this.doc = next;
      this.error = "";
      return true;
    } catch (err) {
      toast.show(String(err).replace(/^invalid: /, ""), "bad");
      return false;
    } finally {
      this.busy = false;
    }
  }

  /** Applies one tempo-map edit (a section tempo, a moved beat line, following the audio...). Same
   *  rules as `edit`: failures are reported, not thrown, and nothing is saved. */
  async tempoEdit(e: TempoEdit): Promise<boolean> {
    const doc = this.doc;
    const l = this.loaded;
    if (!doc || !l || this.busy) return false;
    this.busy = true;
    try {
      const next = await gridVtempoEdit(doc, e, l.sample_rate, l.total_frames > 0 ? l.total_frames : null);
      if (this.doc !== doc || this.loaded !== l) return false; // the track changed under us
      this.#push();
      this.doc = next;
      this.error = "";
      return true;
    } catch (err) {
      toast.show(String(err).replace(/^invalid: /, ""), "bad");
      return false;
    } finally {
      this.busy = false;
    }
  }

  /** Cuts the grid into sections at `frame` (undoable). A cut at a frame already cut does nothing. */
  addSplit(frame: number) {
    if (!this.doc || this.splits.includes(frame)) return;
    this.#push();
    this.splits = [...this.splits, frame].sort((a, b) => a - b);
  }

  removeSplit(frame: number) {
    if (!this.splits.includes(frame)) return;
    this.#push();
    this.splits = this.splits.filter((s) => s !== frame);
  }

  nudgeMs(ms: number) {
    return this.edit({ kind: "nudge", frames: Math.round((ms * this.sampleRate) / 1000) });
  }

  /** Makes the beat nearest `secs` a bar line. */
  setDownbeatAt(secs: number) {
    return this.edit({ kind: "set_downbeat", frame: this.frames(secs) });
  }

  halve() {
    return this.edit({ kind: "halve" });
  }

  double() {
    return this.edit({ kind: "double" });
  }

  setBpm(bpm: number) {
    return this.edit({ kind: "set_bpm", bpm });
  }

  shiftBeats(n: number) {
    return this.edit({ kind: "shift_beats", beats: n });
  }

  /** Replaces the working grid with a fresh analysis of the audio (undoable, not saved). */
  async reanalyse(): Promise<boolean> {
    const lib = session.library;
    const l = this.loaded;
    if (!lib || !l || this.busy) return false;
    this.busy = true;
    try {
      const next = await gridAnalyse(lib.root, l.track_id);
      if (this.loaded !== l) return false;
      this.#push();
      this.doc = next;
      this.error = "";
      return true;
    } catch (err) {
      toast.show(`Analysis failed: ${String(err)}`, "bad");
      return false;
    } finally {
      this.busy = false;
    }
  }

  setCue(secs: number | null) {
    if (!this.loaded) return;
    const frames = secs === null ? null : Math.max(0, this.frames(secs));
    if (frames === this.cue) return;
    this.#push();
    this.cue = frames;
  }

  undo() {
    const prev = this.#past[this.#past.length - 1];
    if (!prev) return;
    this.#future = [...this.#future, { doc: this.doc, cue: this.cue, splits: this.splits }];
    this.#past = this.#past.slice(0, -1);
    this.doc = prev.doc;
    this.cue = prev.cue;
    this.splits = prev.splits;
  }

  redo() {
    const next = this.#future[this.#future.length - 1];
    if (!next) return;
    this.#past = [...this.#past, { doc: this.doc, cue: this.cue, splits: this.splits }];
    this.#future = this.#future.slice(0, -1);
    this.doc = next.doc;
    this.cue = next.cue;
    this.splits = next.splits;
  }

  /** Writes what changed (the .str and/or the cue) and reloads the library list. The caller
   *  has already shown the confirmation. */
  async save(): Promise<boolean> {
    const lib = session.library;
    const l = this.loaded;
    if (!lib || !l || !this.dirty || this.busy) return false;
    if (!jobs.guard()) return false;
    this.busy = true;
    try {
      // the .str may have changed since this track was opened (a re-analysis or add job):
      // refuse to overwrite a grid the user has not seen
      if (this.gridDirty) {
        const now = await gridLoad(lib.root, l.track_id);
        if (JSON.stringify(now.doc) !== JSON.stringify(this.#baseDocState)) {
          const where = session.isComputer ? "in the library" : "on the device";
          this.error = `The beat grid ${where} changed since this track was opened. Re-open the track to see the current grid; nothing was saved.`;
          toast.show(`Grid changed ${where}: nothing saved`, "bad");
          return false;
        }
      }
      // a tempo changed here is the owner's choice, not an analysis result to second-guess
      const bpmChanged = this.gridDirty && Math.abs(trackBpm(this.#baseDocState) - this.bpm) > 0.01;
      const out = await gridSave(
        lib.root,
        l.track_id,
        this.gridDirty ? this.doc : null,
        this.cueDirty ? { frames: this.cue } : null,
      );
      // take the grid back as the file now holds it, so the next save compares like with like
      try {
        const back = await gridLoad(lib.root, l.track_id);
        if (this.loaded === l && back.doc) {
          this.doc = back.doc;
          this.#baseDocState = back.doc;
        } else this.#baseDocState = this.doc;
      } catch {
        this.#baseDocState = this.doc;
      }
      this.#baseCueState = this.cue;
      this.#past = [];
      this.#future = [];
      this.error = "";
      if (out.wrote_grid && bpmChanged) bpmRange.accept([l.track_id]);
      toast.show(out.wrote_grid ? "Grid saved to the .str" : "Cue saved", "ok");
    } catch (err) {
      session.error = `Saving the grid failed: ${String(err)}`;
      return false;
    } finally {
      this.busy = false;
    }
    try {
      await session.load(lib.root, true);
    } catch (err) {
      session.error = `Saved, but the library list could not be reloaded: ${String(err)}`;
    }
    return true;
  }
}

export const editor = new GridEditor();
session.onReset(() => editor.clear());
