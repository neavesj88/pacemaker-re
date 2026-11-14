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
