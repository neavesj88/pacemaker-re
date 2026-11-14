// Batch grid tools: nudge, halve, double, suggested fix and re-analyse on many tracks, with a preview
// that lists every track (old -> new BPM and first beat), the mixes the change would put out of step,
// and an old-vs-new grid view for each track before anything is written. The view that offers them
// (the tracks table's menu, the inspector, the Suspect grid filter) calls `gridBatch.open(ids, kind)`;
// the dialog is views/grid/BatchDialog.svelte. The backend (commands/grid/batch.rs) keeps the grids
// it computed for the preview and the write job saves exactly those, so the preview is the action.
import {
  gridBatchAnalyse,
  gridBatchApply,
  gridBatchCancel,
  gridBatchPlan,
  gridBatchResult,
  type BatchOp,
  type BatchPlan,
  type BatchRow,
} from "$lib/api/grid";
import type { DoneEvent, ProgressEvent } from "$lib/api/types";
import { SvelteSet } from "svelte/reactivity";
import {
  changing,
  mixesWarning,
  mixUse,
  NUDGE_STORAGE,
  opDescription,
  opTitle,
  parseNudge,
  previewKey,
  rowLine,
  type BatchKind,
} from "./batchLogic";
import { suspects } from "./gridSuspects.svelte";
import { jobs } from "./jobs.svelte";
import { session } from "./session.svelte";

/** Job kinds of the analysis behind a re-analyse preview and of the write (commands/grid/batch.rs). */
export const ANALYSE_JOB = "grid-batch-analyse";
export const BATCH_JOB = "grid-batch";

function loadNudge(): string {
  try {
    return localStorage.getItem(NUDGE_STORAGE) || "10";
  } catch {
    return "10";
  }
}

export type Phase = "setup" | "working" | "review";

class GridBatch {
  /** The dialog is on screen (it hides while the write confirmation is up). */
  shown = $state(false);
  ids = $state<number[]>([]);
  kind = $state<BatchKind>("halve");
  /** The typed nudge, in milliseconds. */
  nudgeText = $state(loadNudge());
  phase = $state<Phase>("setup");
  plan = $state<BatchPlan | null>(null);
  /** What the plan was computed for (`previewKey`); differs from the controls' key when they changed since. */
  planKey = $state("");
  /** Tracks the user left ticked for the write. */
  ticked = new SvelteSet<number>();
  /** The row whose old-vs-new view is open. */
  focusId = $state<number | null>(null);
  error = $state("");
  progress = $state<{ done: number; total: number; label: string } | null>(null);
  /** The write confirmation is up; the dialog comes back if it is cancelled. */
  confirming = $state(false);

  nudge = $derived(parseNudge(this.nudgeText));
  // the suggested fix depends on the rules, so changing them makes an earlier preview stale
  key = $derived(previewKey(this.kind, "ms" in this.nudge ? this.nudge.ms : 0, this.ids) + (this.kind === "suggested" ? JSON.stringify(suspects.rules) : ""));
  stale = $derived(this.plan !== null && this.planKey !== this.key);
  rows = $derived(this.plan?.rows ?? []);
  mixes = $derived(mixUse(this.rows, this.ticked));

  /** Opens the dialog for `ids` (tracks of the open library, duplicates dropped). With `kind` the
   *  operation is preselected; "suggested" is previewed at once because it needs no input. */
  open(ids: number[], kind?: BatchKind) {
    const lib = session.library;
    if (!lib) return;
    const known = new Set(lib.tracks.map((t) => t.track_id));
    const list = [...new Set(ids)].filter((id) => known.has(id));
    if (list.length === 0) return;
    this.reset();
    this.ids = list;
    if (kind) this.kind = kind;
    this.shown = true;
    if (kind === "suggested") void this.preview();
  }

  close() {
    if (this.phase === "working") return; // the analysis must be stopped first, so it is not left running unseen
    this.reset();
  }

  reset() {
    this.shown = false;
    this.confirming = false;
    this.phase = "setup";
    this.plan = null;
    this.planKey = "";
    this.ticked.clear();
    this.focusId = null;
    this.error = "";
    this.progress = null;
  }

  /** Back to choosing the operation (keeps the tracks). */
  edit() {
    this.phase = "setup";
    this.plan = null;
    this.planKey = "";
    this.ticked.clear();
    this.focusId = null;
    this.error = "";
  }

  setNudge(text: string) {
    this.nudgeText = text;
    try {
      localStorage.setItem(NUDGE_STORAGE, text);
    } catch {
      // storage blocked: the value is just not remembered
    }
  }

  private opOf(): BatchOp | null {
    switch (this.kind) {
      case "nudge":
        return "ms" in this.nudge ? { kind: "nudge", ms: this.nudge.ms } : null;
      case "halve":
        return { kind: "halve" };
      case "double":
        return { kind: "double" };
      case "suggested":
        return { kind: "suggested", settings: $state.snapshot(suspects.rules) };
      case "reanalyse":
        return null;
    }
  }

  /** Computes the new grids and shows the preview. Re-analysis runs the analyser as a read-only job
   *  with its own progress; the other operations answer at once. Nothing is written. */
  async preview() {
    const lib = session.library;
    if (!lib || this.ids.length === 0 || this.phase === "working") return;
    this.error = "";
    if (this.kind === "nudge" && "error" in this.nudge) {
      this.error = this.nudge.error;
      return;
    }
    const key = this.key;
    if (this.kind === "reanalyse") {
      if (jobs.busy) {
        this.error = "Another job is still running.";
        return;
      }
      this.phase = "working";
      this.progress = { done: 0, total: this.ids.length, label: "" };
      this.planKey = key;
      try {
        await gridBatchAnalyse(lib.root, this.ids);
      } catch (e) {
        this.phase = "setup";
        this.progress = null;
        this.error = String(e);
      }
      return;
    }
    const op = this.opOf();
    if (!op) return;
    try {
      const plan = await gridBatchPlan(lib.root, this.ids, op);
      this.showPlan(plan, key);
    } catch (e) {
      this.error = String(e);
    }
  }

  private showPlan(plan: BatchPlan, key: string) {
    this.plan = plan;
    this.planKey = key;
    this.ticked.clear();
    for (const r of changing(plan.rows)) this.ticked.add(r.track_id);
    this.focusId = changing(plan.rows)[0]?.track_id ?? null;
    this.phase = "review";
    this.progress = null;
  }

  /** Stops the running analysis between tracks; the preview then shows what was analysed. */
  stop() {
    if (this.phase === "working") void gridBatchCancel();
  }

  // ---- job events (views/grid/init.ts) ----

  onProgress(ev: ProgressEvent) {
    if (ev.job !== ANALYSE_JOB || this.phase !== "working") return;
    this.progress = { done: ev.done, total: ev.total, label: ev.label };
  }

  async onDone(ev: DoneEvent) {
    if (ev.job !== ANALYSE_JOB || this.phase !== "working") return;
    const lib = session.library;
    if (ev.error || !lib) {
      this.phase = "setup";
      this.progress = null;
      this.error = ev.error ?? "The analysis could not run.";
      return;
    }
    try {
      const plan = await gridBatchResult(lib.root);
      if (!plan) throw "The analysis left no result. Try again.";
      this.showPlan(plan, this.planKey);
    } catch (e) {
      this.phase = "setup";
      this.progress = null;
      this.error = String(e);
    }
  }

  // ---- ticking ----

  toggle(id: number, on: boolean) {
    if (on) this.ticked.add(id);
    else this.ticked.delete(id);
  }

  tickAll(on: boolean) {
    this.ticked.clear();
    if (on) for (const r of changing(this.rows)) this.ticked.add(r.track_id);
  }

  // ---- the write ----

  /** Opens the write confirmation: one line for every track of the preview (changed, left as it is,
   *  or unticked), then the mixes that would drift. Confirming starts the write job. */
  apply() {
    const lib = session.library;
    const plan = this.plan;
    if (!lib || !plan || this.stale) return;
    const todo = this.rows.filter((r) => r.skip === null && this.ticked.has(r.track_id));
    if (todo.length === 0) return;
    if (!jobs.guard()) return;
    const n = todo.length;
    const note = mixesWarning(mixUse(this.rows, this.ticked));
    const nudgeMs = "ms" in this.nudge ? this.nudge.ms : 0;
    const planId = plan.plan_id;
    const ids = todo.map((r) => r.track_id);
    this.confirming = true;
    const asked = jobs.ask({
      kind: BATCH_JOB,
      root: lib.root,
      count: n,
      names: this.rows.map((r: BatchRow) => rowLine(r, this.ticked.has(r.track_id))),
      title: opTitle(this.kind, n, lib.root, nudgeMs),
      description: opDescription(this.kind, session.library?.kind) + (note ? ` ${note}` : ""),
      confirmLabel: `Change ${n}`,
      runningLabel: "Changing beat grids",
      doneLabel: "Beat grid change finished",
      // only the tracks the confirmation names as changing
      start: () => gridBatchApply(lib.root, planId, ids),
    });
    if (!asked) this.confirming = false;
  }

  /** Called when the write confirmation went away: the write started (close) or was cancelled (show again). */
  confirmationGone() {
    if (!this.confirming) return;
    this.confirming = false;
    if (jobs.job?.running && jobs.job.kind === BATCH_JOB) this.reset();
  }
}

export const gridBatch = new GridBatch();
session.onReset(() => gridBatch.reset());
