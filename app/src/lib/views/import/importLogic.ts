// Pure helpers of the Import view (no Tauri, no Svelte), so node can test them:
//   node --experimental-strip-types --test src/lib/views/import/importLogic.test.mjs
import type { CueAction, CueMode, CueNote } from "$lib/api/import";

/** The same rule the backend applies when the import runs (`CueMode::applies` in pm-formats). */
export const cueApplies = (mode: CueMode, a: CueAction): boolean =>
  (mode === "empty" && a === "set") || (mode === "overwrite" && (a === "set" || a === "replace"));

/** The seconds a cue's stored frames stand for. The note carries the new cue both ways, so the
 *  track's sample rate is not needed. */
export const cueSecs = (c: Pick<CueNote, "cue_frames" | "cue_secs">, frames: number): number =>
  c.cue_frames > 0 ? (frames * c.cue_secs) / c.cue_frames : 0;

/** "1:32.5": a cue position to a tenth of a second. */
export function cueTime(secs: number): string {
  const t = Number.isFinite(secs) ? Math.max(0, Math.floor(secs * 10 + 1e-6)) : 0; // tenths; the epsilon keeps 12.25 * 4 style products exact
  const whole = Math.floor(t / 10);
  return `${Math.floor(whole / 60)}:${String(whole % 60).padStart(2, "0")}.${t % 10}`;
}
