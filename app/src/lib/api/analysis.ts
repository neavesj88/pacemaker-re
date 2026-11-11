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

// ---- energy (auto) ---------------------------------------------------------------------------
// A 1-10 estimate worked out from the audio. The device cannot store it, so results live in an
// app-side file keyed by the library; nothing here writes to the card.

/** One track's estimate. */
export type EnergyEntry = {
  track_id: number;
  /** 1.0 to 10.0, one decimal. */
  energy: number;
  /** The whole level, 1 to 10. */
  level: number;
  /** The 0..1 score before the scale. */
  score: number;
};

export type EnergyScale = "fit" | "fixed";

export type EnergySettings = {
  /** "fit": stretch so the library's own spread covers 1-10; "fixed": the same number whatever else is in the library. */
  scale: EnergyScale;
  /** Percentile of the library's scores that becomes 1 (fit only), 0-40. */
  fit_low_pct: number;
  /** Percentile that becomes 10 (fit only), 60-100. */
  fit_high_pct: number;
  /** Tracks shorter than this many seconds get no estimate shown, 5-600. */
  min_secs: number;
  /** Tracks decoded at once by the job, 1-8. */
  threads: number;
};

export type EnergyState = {
  entries: EnergyEntry[];
  total: number;
  /** Tracks with a number. */
  ready: number;
  /** Too short or silent: no estimate possible. */
  skipped: number;
  /** Analysed, but the file has changed size since. */
  stale: number;
  /** Never analysed. */
  none: number;
  /** The scale actually applied (a fit falls back to "fixed" for a small library). */
  scale_used: EnergyScale;
  anchor_lo: number;
  anchor_hi: number;
  /** Why the fit was not used when it was asked for. */
  note: string | null;
  settings: EnergySettings;
  /** Saved results that could not be read and were dropped. */
  problem: string | null;
};

/** The auto energy of every track of the library that has one, with the counts and the scale in
 * use. The read API for other features: the number is the one the Tracks table shows. It is not
 * the user's own energy rating. */
export const energyState = (root: string) => invoke<EnergyState>("analysis_energy_state", { root });

export const energySettings = () => invoke<EnergySettings>("analysis_energy_settings");

/** Saves the settings (the backend checks every range) and returns what was saved. */
export const saveEnergySettings = (settings: EnergySettings) => invoke<EnergySettings>("analysis_energy_save_settings", { settings });

/** Starts the "energy" background job: decodes tracks and caches what the audio says. Without
 * `trackIds` it covers the library. Without `recompute` only tracks never analysed or changed
 * since are done. Reads the card, writes nothing to it. Normally started through
 * `energy.askAnalyse` (state/energy.svelte.ts), which confirms first. */
export const analyseEnergy = (root: string, trackIds?: number[], recompute = false) =>
  invoke<void>("analysis_energy_analyse", { root, trackIds: trackIds ?? null, recompute });

/** Forgets every saved energy result of the library; returns how many. */
export const clearEnergy = (root: string) => invoke<number>("analysis_energy_clear", { root });

/** The ids a run with these arguments would analyse, in library order: the confirmation lists
 * them, and the run is started with exactly this list. Same selection code as the run. */
export const planEnergy = (root: string, trackIds?: number[], recompute = false) =>
  invoke<number[]>("analysis_energy_plan", { root, trackIds: trackIds ?? null, recompute });
