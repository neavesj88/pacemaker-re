// analysis commands (src-tauri/src/commands/analysis.rs).
import { invoke } from "@tauri-apps/api/core";

/** One waveform column: low, mid, high band level, each 0-255. */
export type Peak = [number, number, number];

/** Three-band waveform of a track resampled to `width` columns (colour waveforms in the track
 * table, grid editor, mix clips and player). Cross-area contract: the analysis area implements
 * `analysis_peaks`; other areas only call this. */
export const peaks = (root: string, location: string, width: number) =>
  invoke<Peak[]>("analysis_peaks", { root, location, width });

/** Starts the "keys" background job: detects the key of tracks and writes `tracks.key`. Without
 * `trackIds` it covers the whole library; tracks that already have a key are left alone unless
 * `overwrite`. A result below `minConfidence` (0..1, default 0) is skipped, not written.
 * Returns once the job has started; progress arrives as job events with `job === "keys"` and the
 * key in `detail`. Normally started through `analysis.askDetectKeys` (state/analysis.svelte.ts),
 * which asks for confirmation first. */
export const detectKeys = (root: string, trackIds?: number[], overwrite = false, minConfidence?: number) =>
  invoke<void>("analysis_detect_keys", { root, trackIds: trackIds ?? null, overwrite, minConfidence: minConfidence ?? null });
