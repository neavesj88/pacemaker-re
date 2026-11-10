// analysis commands (src-tauri/src/commands/analysis.rs).
import { invoke } from "@tauri-apps/api/core";

/** One waveform column: low, mid, high band level, each 0-255. */
export type Peak = [number, number, number];

/** Three-band waveform of a track resampled to `width` columns (colour waveforms in the track
 * table, grid editor, mix clips and player). Cross-area contract: the analysis area implements
 * `analysis_peaks`; other areas only call this. */
export const peaks = (root: string, location: string, width: number) =>
  invoke<Peak[]>("analysis_peaks", { root, location, width });
