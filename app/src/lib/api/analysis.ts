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
 * `overwrite`, which the backend only accepts together with explicit `trackIds`. A result below `minConfidence` (0..1, default 0) is skipped, not written.
 * Returns once the job has started; progress arrives as job events with `job === "keys"` and the
 * key in `detail`. Normally started through `analysis.askDetectKeys` (state/analysis.svelte.ts),
 * which asks for confirmation first. */
export const detectKeys = (root: string, trackIds?: number[], overwrite = false, minConfidence?: number) =>
  invoke<void>("analysis_detect_keys", { root, trackIds: trackIds ?? null, overwrite, minConfidence: minConfidence ?? null });

const BASE: Record<string, number> = { C: 0, D: 2, E: 4, F: 5, G: 7, A: 9, B: 11 };

/** Camelot code ("8A" for Am, "8B" for C) of a key as written in `tracks.key` ("Am", "F#m",
 * "Bb", "A minor"); "" when the text is not a key (already a Camelot code, empty, free text).
 * Same mapping as `Key::camelot` in pm-analysis. For key chips and "compatible with" filters. */
export function camelot(key: string): string {
  const m = /^([A-Ga-g])([#b]?)\s*([A-Za-z]*)$/.exec(key.trim());
  if (!m) return "";
  const suffix = m[3].toLowerCase();
  if (!["", "maj", "major", "m", "min", "minor"].includes(suffix)) return "";
  const minor = suffix.startsWith("m") && !suffix.startsWith("maj");
  const tonic = (BASE[m[1].toUpperCase()] + (m[2] === "#" ? 1 : m[2] === "b" ? 11 : 0)) % 12;
  const number = ((((minor ? tonic + 3 : tonic) % 12) * 7 + 7) % 12) + 1;
  return `${number}${minor ? "A" : "B"}`;
}
