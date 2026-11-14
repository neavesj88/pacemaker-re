/** Which beat ticks the player waveform draws, by how many pixels one beat is wide. */
export type TickTier = "all" | "bars" | "none";

/** The least width of one bar, in px, that still gets a bar line (the Settings choice; 0 = no grid). */
export const GRID_MIN_BAR_PX_DEFAULT = 2;

/**
 * Below ~3 px per beat the beat ticks turn to a smear, so only bar starts are drawn. Below
 * `minBarPx` per bar (an hour-long mix across a narrow bar) even those cover the peaks, so
 * nothing is drawn. `minBarPx` 0 draws no grid at all.
 */
export function tickTier(pxPerBeat: number, minBarPx: number = GRID_MIN_BAR_PX_DEFAULT): TickTier {
  if (!(minBarPx > 0) || pxPerBeat * 4 < minBarPx) return "none";
  if (pxPerBeat < 3) return "bars";
  return "all";
}

/** Bar lines closer than this (px) are drawn faint so they read as a grid, not a wash over the peaks. */
const DENSE_BAR_PX = 4;

/** Alpha for the beat grid lines: faint when bars are packed tight, firmer when there is room. */
export function gridAlpha(pxPerBeat: number): number {
  return pxPerBeat * 4 < DENSE_BAR_PX ? 0.35 : 0.8;
}
