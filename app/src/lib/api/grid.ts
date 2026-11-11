// grid commands (src-tauri/src/commands/grid.rs): the beat grid editor's reads, pure edits,
// re-analysis and the one write (save). Positions are source sample frames, as in the .str.
import { invoke } from "@tauri-apps/api/core";

export type GridBeat = { pos: number; bpm: number };

/** A .str as the editor holds it. */
export type GridDoc = {
  normalize_factor: number;
  beat_start_offset: number;
  beats: GridBeat[];
  /** Index of the first bar line (0-3 once edited), or null when the file marks none. */
  first_downbeat: number | null;
};

export type GridLoad = {
  track_id: number;
  location: string;
  title: string;
  artist: string;
  sample_rate: number;
  duration_secs: number;
  total_frames: number;
  /** Device cue point in source frames; null when unset. */
  cue_frames: number | null;
  /** null when the .str is missing, empty or unreadable; `problem` says why. */
  doc: GridDoc | null;
  problem: string | null;
};

export type GridEdit =
  | { kind: "nudge"; frames: number }
  | { kind: "set_downbeat"; frame: number }
  | { kind: "halve" }
  | { kind: "double" }
  | { kind: "set_bpm"; bpm: number }
  | { kind: "shift_beats"; beats: number };

/** A tempo edit applied to many tracks; the same three edits the grid editor offers. */
export type TempoOp = { kind: "halve" } | { kind: "double" } | { kind: "set"; bpm: number };

/** What a tempo edit will do to one track: `after` is null and `skip` says why when it is left alone. */
export type TempoRow = {
  track_id: number;
  label: string;
  /** The tempo stored now (median of the grid); null when the track has no readable grid. */
  before: number | null;
  after: number | null;
  skip: string | null;
};

export type GridSaved = { wrote_grid: boolean; wrote_cue: boolean };

export const gridLoad = (root: string, trackId: number) => invoke<GridLoad>("grid_load", { root, trackId });

export const gridEdit = (doc: GridDoc, edit: GridEdit, sampleRate: number, totalFrames: number | null) =>
  invoke<GridDoc>("grid_edit", { doc, edit, sampleRate, totalFrames });

/** Analyses the audio again; returns the new grid without saving it. */
export const gridAnalyse = (root: string, trackId: number) => invoke<GridDoc>("grid_analyse", { root, trackId });

/** Writes the grid (`doc`) and/or the cue (`cue.frames`, null clears it); a null part is left alone. */
export const gridSave = (root: string, trackId: number, doc: GridDoc | null, cue: { frames: number | null } | null) =>
  invoke<GridSaved>("grid_save", { root, trackId, doc, cue });

/** What `op` would do to each track, without writing anything. The same code as the job below, so
 * the confirmation lists exactly what then happens. */
export const gridTempoPlan = (root: string, trackIds: number[], op: TempoOp) =>
  invoke<TempoRow[]>("grid_tempo_plan", { root, trackIds, op });

/** Starts the "tempo-edit" write job: each changed track's .str and BPM are rewritten together.
 * Returns once the job has started; progress arrives as job events with `job === "tempo-edit"`.
 * Normally started through `tempo.askHalve/askDouble/askSet` (views/grid/tempo.svelte.ts). */
export const gridTempoApply = (root: string, trackIds: number[], op: TempoOp) =>
  invoke<void>("grid_tempo_apply", { root, trackIds, op });

/** Names of the mixes that contain the track. A mix clip keeps the tempo the track had when the
 * mix was made, so saving a new grid leaves these mixes out of step until they are beatmatched
 * again: the editor lists them and asks before it saves. */
export const gridMixesUsing = (root: string, trackId: number) => invoke<string[]>("grid_mixes_using", { root, trackId });

// ---- suspect grids and batch grid operations (commands/grid/suspect.rs, batch.rs) ----

/** A tempo window for one genre: a track whose genre text contains `name` (any case) is expected
 * between `min` and `max` BPM. */
export type GenreWindow = { name: string; min: number; max: number };

/** The rules that flag a suspect grid. The app keeps them (state/gridSuspects.svelte.ts) and
 * passes them with every call; defaults are in state/suspectDefaults.json. */
export type SuspectSettings = {
  tempo_rule: boolean;
  fallback_min: number;
  fallback_max: number;
  genres: GenreWindow[];
  quality_rule: boolean;
  min_confidence_pct: number;
  steady_tolerance_pct: number;
  coverage_target_pct: number;
};

export type Suspect = {
  track_id: number;
  /** The grid's tempo (median), the number the rules looked at. */
  bpm: number;
  reasons: string[];
  tempo_flag: boolean;
  quality_flag: boolean;
  /** Half or double fits the window; null when neither does (or only the confidence flagged it). */
  fix: "halve" | "double" | null;
  window: [number, number];
  confidence_pct: number | null;
};

export type SuspectReport = { suspects: Suspect[]; checked: number; unreadable: number };

/** Flags the tracks whose tempo is outside its genre window or whose stored grid looks poorly
 * analysed. Read-only. */
export const gridSuspects = (root: string, settings: SuspectSettings) => invoke<SuspectReport>("grid_suspects", { root, settings });

/** A batch operation (re-analysis has its own call, `gridBatchAnalyse`). */
export type BatchOp =
  | { kind: "nudge"; ms: number }
  | { kind: "halve" }
  | { kind: "double" }
  | { kind: "suggested"; settings: SuspectSettings };

export type MixRef = { id: number; name: string };

/** What a batch operation will do to one track. `skip` says why it stays as it is. */
export type BatchRow = {
  track_id: number;
  label: string;
  /** "nudge", "halve", "double" or "reanalyse". */
  op: string;
  before_bpm: number | null;
  after_bpm: number | null;
  /** First beat line in source frames. */
  before_first: number | null;
  after_first: number | null;
  before_beats: number;
  after_beats: number;
  sample_rate: number;
  skip: string | null;
  /** What the analyser said about the new grid (re-analysis only). */
  note: string | null;
  /** Mixes that use the track; a mix clip keeps the tempo the track had when the mix was made.
   * An entry with id -1 means the lookup failed and its name says so. */
  mixes: MixRef[];
};

export type BatchPlan = { plan_id: number; rows: BatchRow[] };

export type BatchDiff = { before: GridDoc | null; after: GridDoc; sample_rate: number; total_frames: number };

/** Computes the new grid of every track for `op` and keeps it under a plan id; nothing is written. */
export const gridBatchPlan = (root: string, trackIds: number[], op: BatchOp) => invoke<BatchPlan>("grid_batch_plan", { root, trackIds, op });

/** Starts the read-only "grid-batch-analyse" job (the analyser on each track's audio). Returns once
 * the job has started; progress and completion are job events; then call `gridBatchResult`. */
export const gridBatchAnalyse = (root: string, trackIds: number[]) => invoke<void>("grid_batch_analyse", { root, trackIds });

/** The plan the analysis job produced; null when there is none for this library. */
export const gridBatchResult = (root: string) => invoke<BatchPlan | null>("grid_batch_result", { root });

/** Stops a running analysis between tracks; the plan then holds the tracks analysed so far. */
export const gridBatchCancel = () => invoke<void>("grid_batch_cancel");

/** Both grids of a planned track, for the old-vs-new view. */
export const gridBatchDiff = (root: string, planId: number, trackId: number) => invoke<BatchDiff>("grid_batch_diff", { root, planId, trackId });

/** Starts the "grid-batch" write job: saves exactly the grids of the plan for `trackIds`. A track whose
 * stored grid changed since the plan was made is skipped. Normally started through
 * `gridBatch.apply()` (state/gridBatch.svelte.ts), which asks for confirmation first. */
export const gridBatchApply = (root: string, planId: number, trackIds: number[]) => invoke<void>("grid_batch_apply", { root, planId, trackIds });
