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

export type GridSaved = { wrote_grid: boolean; wrote_cue: boolean };

export const gridLoad = (root: string, trackId: number) => invoke<GridLoad>("grid_load", { root, trackId });

export const gridEdit = (doc: GridDoc, edit: GridEdit, sampleRate: number, totalFrames: number | null) =>
  invoke<GridDoc>("grid_edit", { doc, edit, sampleRate, totalFrames });

/** Analyses the audio again; returns the new grid without saving it. */
export const gridAnalyse = (root: string, trackId: number) => invoke<GridDoc>("grid_analyse", { root, trackId });

/** Writes the grid (`doc`) and/or the cue (`cue.frames`, null clears it); a null part is left alone. */
export const gridSave = (root: string, trackId: number, doc: GridDoc | null, cue: { frames: number | null } | null) =>
  invoke<GridSaved>("grid_save", { root, trackId, doc, cue });

/** Names of the mixes that contain the track. A mix clip keeps the tempo the track had when the
 * mix was made, so saving a new grid leaves these mixes out of step until they are beatmatched
 * again: the editor lists them and asks before it saves. */
export const gridMixesUsing = (root: string, trackId: number) => invoke<string[]>("grid_mixes_using", { root, trackId });
