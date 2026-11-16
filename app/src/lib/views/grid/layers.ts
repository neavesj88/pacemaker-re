// Pure helpers for the grid editor's canvases, which keep the still parts of a drawing (waveform,
// ruler, beat lines) in an off-screen layer and only blit it and add the moving parts (playhead,
// visible-window box) on each animation frame. No DOM beyond the two fields it reads.

/** Device pixels for a CSS size at a pixel ratio (the same rounding the canvases always used). */
export function bitmapSize(cssW: number, cssH: number, dpr: number): [number, number] {
  return [Math.round(cssW * dpr), Math.round(cssH * dpr)];
}

/** Gives the canvas this bitmap size and says whether it changed. Assigning `width` or `height`,
 *  even to the same value, reallocates the bitmap and clears it, so it is only done on a change. */
export function fitBitmap(c: { width: number; height: number }, w: number, h: number): boolean {
  if (c.width === w && c.height === h) return false;
  c.width = w;
  c.height = h;
  return true;
}
