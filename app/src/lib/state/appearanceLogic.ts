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

/** How long an error message stays, in seconds; 0 = until it is closed. The choices the Settings offer. */
export const ERROR_SECS_CHOICES = [10, 20, 60, 0] as const;
export const ERROR_SECS_DEFAULT = 20;

export function normaliseErrorSecs(raw: unknown): number {
  return finite(raw) && (ERROR_SECS_CHOICES as readonly number[]).includes(raw) ? raw : ERROR_SECS_DEFAULT;
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
export const MAX_TOASTS = 6;
