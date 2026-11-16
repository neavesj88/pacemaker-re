// Double-click on the player's waveform moves the cue flag. The one pure rule here (no Svelte, no
// DOM, so it runs under plain node): which time a pointer position on the waveform stands for.

/** Seconds under the pointer on the player's waveform: it spans `span` seconds starting at
 *  `fromSecs`. Null for a box with no width or no length. */
export function waveSecs(clientX: number, left: number, width: number, fromSecs: number, span: number): number | null {
  if (!(width > 0) || !(span > 0)) return null;
  const f = Math.min(1, Math.max(0, (clientX - left) / width));
  return fromSecs + f * span;
}
