// How tall the bottom player bar is, and how big its contents are at that height. Pure logic so it
// runs under plain node (`playerSize.test.mjs`); the bar (Player.svelte) owns the pointer events and
// the storage (prefStore).
//
// The remembered number is what the owner chose. The height actually shown is that number clamped
// to the current window, so a tall bar chosen on a 5120x2160 screen still fits a 1080 one without
// the choice being overwritten (only an explicit drag, key press or reset changes what is saved).

/** localStorage key of the remembered height (the one preference this bar has). */
export const PLAYER_HEIGHT_KEY = "pacemaker-revival.playerHeight";

/** The bar's height before it is resized: 1px top border, 6px padding twice, 50px waveform box. */
export const DEFAULT_HEIGHT = 63;
/** Never shorter than the default. */
export const MIN_HEIGHT = DEFAULT_HEIGHT;
/** The bar may take this share of the window height, so the view above it stays usable. */
export const MAX_WINDOW_SHARE = 0.45;
/** Sanity ceiling for a stored value (a corrupt number cannot ask for a screen-high bar). */
export const HARD_MAX_HEIGHT = 1200;

/** Content scale moves in steps of this size, up to the ceiling. */
export const SCALE_STEP = 0.25;
export const MAX_SCALE = 4;
/** The content grows at half the rate of the bar, so the waveform takes the rest of the extra height. */
const SCALE_PER_HEIGHT = 0.5;
/**
 * Width of everything beside the waveform at scale 1 (title column 260, controls 377, two gaps,
 * side padding), measured in the 1920 px preview; the waveform keeps at least the other 45 %
 * of the window.
 */
export const SIDE_WIDTH_AT_1 = 690;
const SIDE_SHARE = 0.55;

const finite = (n: unknown): n is number => typeof n === "number" && Number.isFinite(n);

/** Tallest the bar may be in a window `winH` px high (never below the minimum). */
export function maxHeight(winH: number): number {
  const w = finite(winH) && winH > 0 ? winH : 0;
  return Math.max(MIN_HEIGHT, Math.floor(w * MAX_WINDOW_SHARE));
}

/** `h` held between the minimum and what a `winH` px window allows. */
export function clampHeight(h: number, winH: number): number {
  if (!finite(h)) return DEFAULT_HEIGHT;
  return Math.round(Math.min(maxHeight(winH), Math.max(MIN_HEIGHT, h)));
}

/** Reads a saved value (also the `normalise` for loadPref): nothing, junk or out of range gives a usable number. */
export function normaliseHeight(raw: unknown): number {
  if (!finite(raw)) return DEFAULT_HEIGHT;
  return Math.round(Math.min(HARD_MAX_HEIGHT, Math.max(MIN_HEIGHT, raw)));
}

/**
 * Content scale (text, buttons, volume) for a bar `h` px high in a window `winW` px wide: 1 at the
 * default, rising in quarter steps, held back so the title column and controls leave the waveform
 * most of the width. Never below 1.
 */
export function scaleFor(h: number, winW: number): number {
  const byHeight = 1 + ((h - DEFAULT_HEIGHT) / DEFAULT_HEIGHT) * SCALE_PER_HEIGHT;
  const byWidth = finite(winW) && winW > 0 ? (winW * SIDE_SHARE) / SIDE_WIDTH_AT_1 : 1;
  const s = Math.min(byHeight, byWidth, MAX_SCALE);
  return Math.max(1, Math.floor(s / SCALE_STEP + 1e-9) * SCALE_STEP);
}

/** Height after a drag: the handle is on the top edge, so moving up (smaller clientY) grows the bar. */
export function dragHeight(startH: number, startY: number, y: number, winH: number): number {
  return clampHeight(startH + (startY - y), winH);
}

export const KEY_STEP = 8;
export const KEY_STEP_BIG = 32;

/** New height for a key on the handle (Up grows, Down shrinks, Shift = bigger step, Home = min, End = max), or null for other keys. */
export function keyHeight(key: string, shift: boolean, h: number, winH: number): number | null {
  const step = shift ? KEY_STEP_BIG : KEY_STEP;
  switch (key) {
    case "ArrowUp":
      return clampHeight(h + step, winH);
    case "ArrowDown":
      return clampHeight(h - step, winH);
    case "Home":
      return MIN_HEIGHT;
    case "End":
      return maxHeight(winH);
    default:
      return null;
  }
}
