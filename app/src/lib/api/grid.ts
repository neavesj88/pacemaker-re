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
