// Background write jobs of any kind: the confirmation that precedes any write, the running job
// and its progress. An area starts its own kind of job with `jobs.ask({...})`; nothing here
// knows what a job does. `add` / `reanalyse` below are adapters built on the same entry point.
// Writes happen only through `confirm()`.
import type { DoneEvent, JobKind, ProgressEvent } from "$lib/api/types";
import { addTracks, expandAudioPaths, pickAudioFiles } from "$lib/api/import";
import { reanalyseTracks } from "$lib/api/tracks";
import { baseName } from "$lib/format";
import { mixesNote } from "./mixNote";
import { session } from "./session.svelte";

export type Job = {
  kind: JobKind;
  /** "Adding", "Analysing": the strip's heading while running. */
  runningLabel: string;
  /** "Add finished": the strip's heading when done. */
  doneLabel: string;
  running: boolean;
  total: number;
  done: number;
  current: string;
  bpm: number | null;
  failures: { label: string; error: string }[];
  result: DoneEvent | null;
};

/** What the user is asked to confirm before anything is written to the device. Built by the
 *  area that owns the job; `start` is called on confirm and must start the backend job (it
 *  returns once the job has started; progress and completion arrive as events). */
export type JobRequest = {
  kind: JobKind;
  root: string;
  count: number;
  names: string[];
  title: string;
  description: string;
  confirmLabel: string;
  runningLabel: string;
  doneLabel: string;
  start: () => Promise<void>;
};

class Jobs {
  job = $state<Job | null>(null);
  pending = $state<JobRequest | null>(null);
  /** An OS drag is hovering over the window. */
  dragging = $state(false);

  busy = $derived(this.job?.running ?? false);
  canWrite = $derived(!!session.library && !session.isDetached && session.library.problems.length === 0 && !this.busy);

  /** Opens the confirmation for a job. Returns false (and sets the error banner) if writing is
   *  not possible right now. */
  ask(req: JobRequest): boolean {
    if (!this.guard()) return false;
    session.error = "";
    this.pending = req;
    return true;
  }

  /** True if a write can start now; otherwise sets the error banner saying why. */
  guard(): boolean {
    if (this.canWrite) return true;
    session.error = this.busy
      ? "Another job is still running."
      : session.isDetached
        ? "This Pacemaker was ejected, so nothing can be written to it. Plug it in and open it again, or choose another library."
        : "This library is not device-compatible, so it stays read-only here.";
    return false;
  }

  cancel() {
    this.pending = null;
  }

  dismiss() {
    this.job = null;
  }

  /** Starts the pending job. True when it started; false when there was nothing pending or the
   *  backend refused to start it (the error banner says why). */
  async confirm(): Promise<boolean> {
    const p = this.pending;
    this.pending = null;
    if (!p) return false;
    this.job = {
      kind: p.kind,
      runningLabel: p.runningLabel,
      doneLabel: p.doneLabel,
      running: true,
      total: p.count,
      done: 0,
      current: "",
      bpm: null,
      failures: [],
      result: null,
    };
    try {
      await p.start();
      return true;
    } catch (e) {
      this.job = null;
      session.error = String(e);
      return false;
    }
  }

  /** Called from the shell's job-event subscriptions (views/import/init.ts). */
  onProgress(ev: ProgressEvent) {
    const job = this.job;
    if (!job || job.kind !== ev.job) return;
    job.done = ev.done;
    job.total = ev.total;
    job.current = ev.label;
    job.bpm = ev.bpm;
    if (ev.status === "failed") job.failures.push({ label: ev.label, error: ev.error ?? "failed" });
  }

  async onDone(ev: DoneEvent) {
    const job = this.job;
    if (!job || job.kind !== ev.job) return;
    job.running = false;
    job.result = ev;
    if (ev.error) session.error = ev.error;
    if (session.library) await session.load(session.library.root, true);
  }

  // ---- adapters for the two built-in kinds ----

  async pickFiles() {
    const files = await pickAudioFiles();
    if (files.length) await this.askAdd(files);
  }

  /** Confirm adding these picked / dropped paths; folders are searched, non-audio is dropped.
   *  `opts.source` names where the files came from (the confirmation says so); `opts.auto` starts
   *  the add without the question (only the watch-folder "add automatically" setting passes it).
   *  Returns true when the add was set up (asked, or really started for `auto`). */
  async askAdd(paths: string[], opts: { source?: string; auto?: boolean } = {}): Promise<boolean> {
    const lib = session.library;
    if (!lib) return false;
    if (!this.guard()) return false;
    const files = await expandAudioPaths(paths);
    if (files.length === 0) {
      session.error = "None of those are audio files (mp3, flac, wav, m4a, aac, ogg, aiff).";
      return false;
    }
    // a background find must never replace a question the user is looking at (checked after the
    // await above, when it could have appeared meanwhile)
    if (opts.source && (this.pending || !this.guard())) return false;
    const n = files.length;
    const from = opts.source ? ` from ${opts.source}` : "";
    const asked = this.ask({
      kind: "add",
      root: lib.root,
      count: n,
      names: files.map(baseName),
      title: `Add ${n} track${n === 1 ? "" : "s"}${from} to ${lib.root} ?`,
      description: session.isComputer
        ? "The files are analysed, copied into the library on this computer and added to it. Tracks already there are skipped."
        : "The files are analysed, copied onto the device and added to its library. Tracks already there are skipped.",
      confirmLabel: `Add ${n}`,
      runningLabel: "Adding",
      doneLabel: "Add finished",
      start: () => addTracks(lib.root, files),
    });
    if (asked && opts.auto) return await this.confirm();
    return asked;
  }

  /** Confirm re-analysing `only` (any tracks, for example the selection) or, without it, every
   *  track that needs analysis. */
  async askReanalyse(only?: number[]) {
    const lib = session.library;
    if (!lib) return;
    const ids = only ? [...new Set(only)].filter((id) => lib.tracks.some((t) => t.track_id === id)) : [...session.needs.keys()];
    if (ids.length === 0) return;
    const n = ids.length;
    const wanted = new Set(ids);
    // a mix clip keeps the tempo its track had when the mix was made, so a new grid and tempo
    // leaves those mixes out of step: name them in the question
    const mixNote = await mixesNote(lib.root, ids);
    // tracks picked by hand may have a grid or BPM that someone set on purpose
    const handNote = only ? " A beat grid or BPM you set by hand for these tracks is replaced by the new analysis." : "";
    this.ask({
      kind: "reanalyse",
      root: lib.root,
      count: n,
      names: lib.tracks.filter((t) => wanted.has(t.track_id)).map((t) => t.title || baseName(t.location)),
      title: `Re-analyse ${n} track${n === 1 ? "" : "s"} on ${lib.root} ?`,
      description:
        `Only the beat grid (.str) and BPM of these tracks are rewritten, from the audio already ${session.isComputer ? "in the library" : "on the device"}. No audio is copied.` +
        handNote +
        mixNote,
      confirmLabel: `Analyse ${n}`,
      runningLabel: "Analysing",
      doneLabel: "Analysis finished",
      start: () => reanalyseTracks(lib.root, ids),
    });
  }
}

export const jobs = new Jobs();
