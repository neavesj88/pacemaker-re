// The player and mix preview volume (one slider value, 0 to 1) that is remembered between runs, so
// a restart does not start at a louder level than the one the owner last chose. Plain TypeScript so
// it runs under plain node (`volumePref.test.mjs`); playback.svelte.ts owns the storage calls.

/** localStorage key of the remembered volume. */
export const VOLUME_KEY = "pacemaker-revival.volume";
/** Where the volume starts when nothing usable is remembered. */
export const DEFAULT_VOLUME = 0.8;

/** A saved value (also the `normalise` for loadPref): nothing, junk or out of range gives a usable level. */
export function normaliseVolume(raw: unknown): number {
  if (typeof raw !== "number" || !Number.isFinite(raw)) return DEFAULT_VOLUME;
  return Math.min(1, Math.max(0, raw));
}
