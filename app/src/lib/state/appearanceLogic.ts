// How big the interface is and how long an error message stays. Pure, so it runs under plain node
// (appearanceLogic.test.mjs); the state that remembers them is appearance.svelte.ts.

/** localStorage keys (per computer, like the player bar height: see prefStore.ts). */
export const SCALE_KEY = "pacemaker-revival.interfaceSize";
export const ERROR_SECS_KEY = "pacemaker-revival.errorSeconds";

export const SCALE_MIN = 80;
export const SCALE_MAX = 200;
export const SCALE_STEP = 10;
export const SCALE_DEFAULT = 100;

const finite = (n: unknown): n is number => typeof n === "number" && Number.isFinite(n);

/** A whole per cent held between the smallest and the largest size. Junk gives the default. */
export function clampScale(pct: unknown): number {
  if (!finite(pct)) return SCALE_DEFAULT;
  return Math.min(SCALE_MAX, Math.max(SCALE_MIN, Math.round(pct)));
}

/** The saved value (also the `normalise` for loadPref). */
export const normaliseScale = (raw: unknown): number => clampScale(raw);

/** One step bigger (`dir` 1) or smaller (-1), landing on a multiple of the step, so 105 % goes to 110 and 100. */
export function stepScale(pct: number, dir: 1 | -1): number {
  const p = clampScale(pct);
  const next = dir > 0 ? (Math.floor(p / SCALE_STEP) + 1) * SCALE_STEP : (Math.ceil(p / SCALE_STEP) - 1) * SCALE_STEP;
  return clampScale(next);
}

/** The factor the webview takes (1 = 100 %). */
export const zoomFactor = (pct: number): number => clampScale(pct) / 100;

/** The shortcut a key press means: Ctrl+= / Ctrl++ bigger, Ctrl+- smaller, Ctrl+0 back to 100 %. Null for anything else. */
export function scaleKey(e: { key: string; ctrlKey: boolean; metaKey: boolean; altKey: boolean }): "bigger" | "smaller" | "reset" | null {
  if (!(e.ctrlKey || e.metaKey) || e.altKey) return null;
  if (e.key === "=" || e.key === "+") return "bigger";
  if (e.key === "-" || e.key === "_") return "smaller";
  if (e.key === "0") return "reset";
  return null;
}

/** The beat grid on the player waveform: the least width of one bar (px) that still gets a bar line. 0 = no grid. */
export const GRID_MIN_KEY = "pacemaker-revival.waveGridMinBarPx";
export const GRID_MIN_CHOICES = [0, 2, 4, 8] as const;
export const GRID_MIN_DEFAULT = 2;

export function normaliseGridMin(raw: unknown): number {
  return finite(raw) && (GRID_MIN_CHOICES as readonly number[]).includes(raw) ? raw : GRID_MIN_DEFAULT;
}

/** The colours of the three-band waveforms (low, mid, high: player bar, track pictures, the mix editor's overview,
 *  grid editor). Matrix green, calm (the app's own green in three quiet steps, darkest outside) is
 *  the default; Neon is the original pink, lilac and bright green. */
export const WAVE_THEME_KEY = "pacemaker-revival.waveColours";
export const WAVE_THEMES = ["matrix", "neon"] as const;
export type WaveTheme = (typeof WAVE_THEMES)[number];
export const WAVE_THEME_DEFAULT: WaveTheme = "matrix";
export const WAVE_COLOURS: Record<WaveTheme, { low: string; mid: string; high: string }> = {
  matrix: { low: "#1d6b3f", mid: "#3fae66", high: "#bfeccd" },
  neon: { low: "#ff48a8", mid: "#e070ff", high: "#00ff46" },
};
export const WAVE_THEME_NAMES: Record<WaveTheme, string> = { matrix: "Matrix green, calm", neon: "Neon" };

export function normaliseWaveTheme(raw: unknown): WaveTheme {
  return (WAVE_THEMES as readonly unknown[]).includes(raw) ? (raw as WaveTheme) : WAVE_THEME_DEFAULT;
}

/** How often the playhead and the time readouts are redrawn while a track plays, per second at most;
 *  0 = at every refresh of the screen. A 144 or 166 Hz screen drew the playhead that often and the
 *  page cost about twice the processor time of a 60 Hz one for a move nobody can tell apart, so the
 *  default stops at 60. The sound and the position the player keeps are not affected. */
export const PLAYHEAD_FPS_KEY = "pacemaker-revival.playheadFps";
export const PLAYHEAD_FPS_CHOICES = [60, 30, 0] as const;
export const PLAYHEAD_FPS_DEFAULT = 60;

export function normalisePlayheadFps(raw: unknown): number {
  return finite(raw) && (PLAYHEAD_FPS_CHOICES as readonly number[]).includes(raw) ? raw : PLAYHEAD_FPS_DEFAULT;
}

/** Whether the animation frame at `now` (ms) should redraw the playhead, given the last redraw
 *  at `last`. A frame counts once three quarters of the interval has passed, so a 60 Hz screen
 *  under the 60 cap draws every frame, a 144 Hz one about every second frame (72 a second), a
 *  166 Hz one every third (55) and the 30 cap on a 60 Hz screen every second frame. */
export function playheadDue(now: number, last: number, fps: number): boolean {
  if (fps <= 0) return true;
  return now - last >= (1000 / fps) * 0.75;
}

/** How long an error message stays, in seconds; 0 = until it is closed. The choices the Settings offer. */
export const ERROR_SECS_CHOICES = [10, 20, 60, 0] as const;
export const ERROR_SECS_DEFAULT = 20;

export function normaliseErrorSecs(raw: unknown): number {
  return finite(raw) && (ERROR_SECS_CHOICES as readonly number[]).includes(raw) ? raw : ERROR_SECS_DEFAULT;
}

/** How long the note of a finished add or analysis stays above the views when it went well, in
 *  seconds; 0 = until it is dismissed. A note with failures in it, or with an Undo, always stays. */
export const DONE_SECS_KEY = "pacemaker-revival.finishedNoteSeconds";
export const DONE_SECS_CHOICES = [5, 10, 30, 0] as const;
export const DONE_SECS_DEFAULT = 10;

export function normaliseDoneSecs(raw: unknown): number {
  return finite(raw) && (DONE_SECS_CHOICES as readonly number[]).includes(raw) ? raw : DONE_SECS_DEFAULT;
}

/** How long a toast stays on screen in ms, or null for until it is closed. A message about a
 *  failure is read at the user's own pace (`errorSecs`, never shorter than the caller asked for);
 *  the rest keep the caller's time. */
export function toastMs(kind: "info" | "ok" | "bad", ms: number, errorSecs: number): number | null {
  if (kind !== "bad") return ms;
  if (errorSecs === 0) return null;
  return Math.max(ms, errorSecs * 1000);
}

/** The most toasts on screen at once; the oldest goes first, so a closed-by-hand stack cannot grow without end. */
export const MAX_TOASTS = 4;

/** The stack after a message is added. The same message again (same text and kind) replaces its
 *  earlier card and counts it ("x3"), so adding ten tracks one by one is one card, not a wall. */
export function addToast<T extends { id: number; text: string; kind: string; count: number }>(items: readonly T[], next: Omit<T, "count">, max = MAX_TOASTS): T[] {
  const same = items.find((t) => t.text === next.text && t.kind === next.kind);
  const rest = items.filter((t) => t !== same);
  return [...rest, { ...next, count: (same?.count ?? 0) + 1 } as T].slice(-max);
}
