/** Which beat ticks the player waveform draws, by how many pixels one beat is wide. */
export type TickTier = "all" | "bars" | "none";

/**
 * Below ~3 px per beat the beat ticks turn to a smear, so only bar starts are drawn. Below
 * ~6 px per bar (a long mix across a narrow bar) even those cover the peaks, so nothing is drawn.
 */
export function tickTier(pxPerBeat: number): TickTier {
  if (pxPerBeat * 4 < 6) return "none";
  if (pxPerBeat < 3) return "bars";
  return "all";
}
