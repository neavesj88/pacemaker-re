// Draws three-band peaks (low, mid, high; 0-255 each) on a canvas, coloured by frequency like the
// approved mockup: three bands, darkest outside. The colours are the Waveform colours setting
// (calm matrix green by default, or neon), read here so a picture redraws when it changes.
import type { Peak } from "$lib/api/analysis";
import { appearance } from "$lib/state/appearance.svelte";

const waveColours = () => appearance.waveBands;

/** Paints `peaks` over the whole canvas (CSS size cssW x cssH), replacing what was there. With
 *  more peaks than pixels the loudest in each pixel column is drawn. */
export function drawPeaks(canvas: HTMLCanvasElement, peaks: Peak[] | null, cssW: number, cssH: number) {
  const dpr = window.devicePixelRatio || 1;
  canvas.width = Math.max(1, Math.round(cssW * dpr));
  canvas.height = Math.max(1, Math.round(cssH * dpr));
  const g = canvas.getContext("2d");
  if (!g) return;
  g.setTransform(dpr, 0, 0, dpr, 0, 0);
  g.clearRect(0, 0, cssW, cssH);
  if (!peaks || peaks.length === 0) return;
  const c = waveColours();
  const n = peaks.length;
  const cols = Math.min(n, Math.max(1, Math.floor(cssW)));
  const step = cssW / cols;
  const mid = cssH / 2;
  const half = mid - 1;
  for (let i = 0; i < cols; i++) {
    const from = Math.floor((i * n) / cols);
    const to = Math.max(from + 1, Math.floor(((i + 1) * n) / cols));
    let lo = 0;
    let md = 0;
    let hi = 0;
    for (let j = from; j < to; j++) {
      lo = Math.max(lo, peaks[j][0]);
      md = Math.max(md, peaks[j][1]);
      hi = Math.max(hi, peaks[j][2]);
    }
    const x = i * step;
    const w = Math.max(1, step);
    const l = (lo / 255) * half;
    const m = (md / 255) * half * 0.75;
    const h = (hi / 255) * half * 0.45;
    g.fillStyle = c.low;
    g.fillRect(x, mid - l, w, Math.max(1, l * 2));
    g.fillStyle = c.mid;
    g.fillRect(x, mid - m, w, Math.max(1, m * 2));
    g.fillStyle = c.high;
    g.fillRect(x, mid - h, w, Math.max(1, h * 2));
  }
}
