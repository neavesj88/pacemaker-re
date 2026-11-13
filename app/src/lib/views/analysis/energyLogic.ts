// Pure logic of the energy (auto) feature: wording, settings limits, lookups. No store or
// component imports (types only) so it can be tested with plain node:
// `node --experimental-strip-types src/lib/views/analysis/energyLogic.test.mjs`.
import type { EnergyEntry, EnergySettings, EnergyState } from "../../api/analysis";

export const DEFAULT_SETTINGS: EnergySettings = { scale: "fit", fit_low_pct: 5, fit_high_pct: 95, min_secs: 20, threads: 2 };

/** The ranges the backend enforces (`Settings::validate` in commands/analysis/energy.rs). */
export const LIMITS = {
  fit_low_pct: { min: 0, max: 40 },
  fit_high_pct: { min: 60, max: 100 },
  min_secs: { min: 5, max: 600 },
  threads: { min: 1, max: 8 },
} as const;

export type NumericSetting = keyof typeof LIMITS;

/** `value` forced into the setting's range (whole numbers); NaN becomes the default. */
export function clampSetting(key: NumericSetting, value: number): number {
  if (!Number.isFinite(value)) return DEFAULT_SETTINGS[key];
  const { min, max } = LIMITS[key];
  return Math.min(max, Math.max(min, Math.round(value)));
}

export function sameSettings(a: EnergySettings, b: EnergySettings): boolean {
  return a.scale === b.scale && a.fit_low_pct === b.fit_low_pct && a.fit_high_pct === b.fit_high_pct && a.min_secs === b.min_secs && a.threads === b.threads;
}

export function energyById(entries: readonly EnergyEntry[]): Map<number, EnergyEntry> {
  return new Map(entries.map((e) => [e.track_id, e]));
}

/** Width of a meter bar for an energy of 1 to 10, as a percentage (1 is a sliver, 10 is full). */
export function meterPercent(energy: number): number {
  const e = Math.min(10, Math.max(1, energy));
  return Math.round(((e - 1) / 9) * 90 + 10);
}

/** "7.4" for the table; one decimal always, so a column lines up. */
export const energyLabel = (energy: number) => energy.toFixed(1);

export function cellTitle(e: EnergyEntry): string {
  return `Energy ${energyLabel(e.energy)} of 10, estimated from the audio (score ${e.score.toFixed(2)}). Kept by this app only: it is not written to the device.`;
}

/** Tracks a default run would analyse: never analysed, or the file has changed since. */
export const toAnalyse = (s: EnergyState): number => s.none + s.stale;

const plural = (n: number, one: string, many = `${one}s`) => `${n} ${n === 1 ? one : many}`;

/** The sentence under the panel's title. */
export function coverageText(s: EnergyState): string {
  if (s.total === 0) return "This library has no tracks.";
  const parts = [`${s.ready} of ${plural(s.total, "track")} ${s.ready === 1 ? "has" : "have"} an energy estimate.`];
  if (s.none > 0) parts.push(`${s.none} not analysed yet.`);
  if (s.stale > 0) parts.push(`${plural(s.stale, "file")} changed since ${s.stale === 1 ? "it was" : "they were"} analysed.`);
  if (s.skipped > 0) parts.push(`${s.skipped} too short, silent or under the minimum length, so without one.`);
  return parts.join(" ");
}

/** What the scale in force means, in words. */
export function scaleText(s: EnergyState): string {
  if (s.scale_used === "fit") {
    return `Fitted to this library: scores from ${s.anchor_lo.toFixed(2)} (energy 1) to ${s.anchor_hi.toFixed(2)} (energy 10).`;
  }
  return `Fixed scale: scores from ${s.anchor_lo.toFixed(2)} (energy 1) to ${s.anchor_hi.toFixed(2)} (energy 10), the same in every library.`;
}

/** Title of the confirmation of a run. */
export function runTitle(count: number, root: string, recompute: boolean): string {
  return `${recompute ? "Analyse again" : "Analyse the energy of"} ${plural(count, "track")} on ${root} ?`;
}

export const RUN_DESCRIPTION =
  "Each track is decoded and measured for loudness, attack density and low-end weight. Nothing is written to the device or to the audio files: the results are kept by this app, per library, and the numbers do not appear on the device. Estimates from audio are a good guide to relative intensity, not a measurement of how a track feels.";
