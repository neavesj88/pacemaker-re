// Draws three-band peaks (low, mid, high; 0-255 each) on a canvas, coloured by frequency like the
// approved mockup: bass in the accent colour, mids lighter, highs in the matrix green. Colours
// come from the palette tokens, resolved once from the page's computed style.
import type { Peak } from "$lib/api/analysis";

type Colours = { low: string; mid: string; high: string };
let colours: Colours | null = null;

function resolve(expr: string): string {
  const probe = document.createElement("span");
  probe.style.color = expr;
  document.body.appendChild(probe);
  const out = getComputedStyle(probe).color;
  probe.remove();
  return out;
}

function waveColours(): Colours {
  if (!colours) {
    const low = resolve("var(--accent)");
    colours = { low, mid: resolve("color-mix(in srgb, var(--accent) 55%, white)") || low, high: resolve("var(--green)") };
  }
  return colours;
}

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
