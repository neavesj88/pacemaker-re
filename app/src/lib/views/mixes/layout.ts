// Pure layout helpers of the mix editor's timeline: zoom, ruler ticks, clocks, row packing,
// tool colours, curve presets, envelope adapters. No Svelte, no DOM, no value imports from the
// app (type imports only), so it runs under plain node (layout.test.mjs).
import type { CurveNode, EnvNode, KeyNode } from "$lib/api/mixes";

/** Engine sample rate (same as SR in state/mixedit.ts; duplicated so this file imports no value). */
export const SR = 44100;
/** The empty room the timeline keeps after the end of the mix (or the playhead), samples. */
export const END_ROOM = 45 * SR;

export const PPS_MIN = 0.05;
export const PPS_MAX = 400;
export const HEADER_W = 132;

export const clamp = (x: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, x));
export const toSec = (samples: number) => samples / SR;
export const toSamples = (sec: number) => Math.round(sec * SR);
/** Pixels of a sample count at `pps` pixels per second. */
export const toPx = (samples: number, pps: number) => (samples / SR) * pps;
/** Samples at pixel `px`. */
export const fromPx = (px: number, pps: number) => (px / pps) * SR;

/** Width of the scrollable lanes (everything right of the lane headers): the viewport at least,
 * else the end of the content (`endSamples`: mix end or playhead, plus the empty room after it). */
export const laneContentW = (endSamples: number, pps: number, viewW: number): number => Math.max(Math.max(0, viewW - HEADER_W), toPx(endSamples, pps));

/** The furthest the lanes can scroll: 0 while everything fits. A scroll position beyond it is one
 * the browser clamps away, so a view state holding it no longer matches the screen. */
export const maxScroll = (endSamples: number, pps: number, viewW: number): number => Math.max(0, laneContentW(endSamples, pps, viewW) - Math.max(0, viewW - HEADER_W));

/** The lanes' scroll position after a zoom from `pps` to `next`, keeping the time under `anchorPx`
 * (a pixel inside the lanes' viewport) where it is, and never past what the new zoom can scroll. */
export function zoomScroll(scrollLeft: number, pps: number, next: number, anchorPx: number, endSamples: number, viewW: number): number {
  const at = (scrollLeft + anchorPx) / pps;
  return clamp(at * next - anchorPx, 0, maxScroll(endSamples, next, viewW));
}

/** The part of a clip to draw a waveform canvas for, in pixels from the clip's left edge: the lanes'
 * viewport plus half a screen each side, widened to whole `step`s so scrolling redraws rarely.
 * `clipLeft` is the clip's left edge in lane pixels, `clipW` its width. At least 1 px wide; a clip wholly off screen gets an empty window past its end. */
export function waveWindow(scrollLeft: number, viewW: number, clipLeft: number, clipW: number, step: number): { from: number; to: number } {
  const vl = scrollLeft - viewW / 2 - clipLeft;
  const vr = scrollLeft + viewW - HEADER_W + viewW / 2 - clipLeft;
  const from = Math.max(0, Math.floor(vl / step) * step);
  const to = Math.min(clipW, Math.ceil(vr / step) * step);
  return { from, to: Math.max(from + 1, to) };
}

/** Zoom slider (0..100) to pixels per second: logarithmic, so every step feels alike. */
export function ppsFromSlider(v: number): number {
  return PPS_MIN * Math.pow(PPS_MAX / PPS_MIN, clamp(v, 0, 100) / 100);
}
export function sliderFromPps(pps: number): number {
  return (Math.log(clamp(pps, PPS_MIN, PPS_MAX) / PPS_MIN) / Math.log(PPS_MAX / PPS_MIN)) * 100;
}

/** "m:ss.t" clock text. */
export function clock(sec: number): string {
  const s = Math.max(0, sec);
  const m = Math.floor(s / 60);
  const r = s - m * 60;
  const whole = Math.floor(r);
  const tenth = Math.floor((r - whole) * 10);
  return `${m}:${String(whole).padStart(2, "0")}.${tenth}`;
}

/** "m:ss" label (no tenths) for ruler ticks and lists. */
export function mmss(sec: number): string {
  const s = Math.max(0, Math.round(sec));
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
}

/** Bar and beat of a timeline sample at `bpm` (1-based, 4/4 from timeline 0). */
export function barBeat(samples: number, bpm: number): { bar: number; beat: number } {
  // a sample of slack: a clip snapped onto a bar line is stored rounded to a whole sample
  const beat = (Math.max(0, samples) + 1) / ((60 / bpm) * SR);
  return { bar: Math.floor(beat / 4) + 1, beat: Math.floor(beat % 4) + 1 };
}

const TIME_STEPS = [0.5, 1, 2, 5, 10, 15, 30, 60, 120, 300, 600, 1800];
const BAR_STEPS = [1, 2, 4, 8, 16, 32, 64, 128, 256, 512, 1024, 2048, 4096, 8192];
/** Room a bar number needs on the ruler: its digits (about 7 px each) and a gap. */
export const barLabelPx = (barNumber: number): number => String(Math.max(1, Math.floor(barNumber))).length * 7 + 26;

export type RulerTicks = {
  /** Labelled time marks: seconds and text. */
  time: { sec: number; label: string }[];
  /** Bar lines at the mix tempo; `label` (the bar number) only where there is room. */
  bars: { sec: number; bar: number; label: boolean }[];
  /** Beat lines (only when they are at least 7 px apart). */
  beats: number[];
};

/** Ruler marks between two times (seconds). Marks fall on the grid of the whole timeline, so
 * they do not shift as the view scrolls. */
export function rulerTicks(fromSec: number, toSecs: number, pps: number, bpm: number): RulerTicks {
  const time: RulerTicks["time"] = [];
  const step = TIME_STEPS.find((s) => s * pps >= 74) ?? TIME_STEPS[TIME_STEPS.length - 1];
  for (let t = Math.max(0, Math.ceil(fromSec / step)) * step; t <= toSecs; t += step) {
    time.push({ sec: t, label: step < 1 ? clock(t) : mmss(t) });
  }
  const bars: RulerTicks["bars"] = [];
  const beats: number[] = [];
  if (bpm > 0 && Number.isFinite(bpm)) {
    const beatSec = 60 / bpm;
    const barSec = beatSec * 4;
    const barPx = barSec * pps;
    // a bar line every `skip` bars once bars get too dense to draw
    const skip = barPx >= 7 ? 1 : (BAR_STEPS.find((n) => n * barPx >= 7) ?? BAR_STEPS[BAR_STEPS.length - 1]);
    // the widest number on screen sets the spacing, so four-digit bar numbers at the far zoom-out do not touch
    const labelPx = barLabelPx(toSecs / barSec + 1);
    const labelEvery = BAR_STEPS.find((n) => n * barPx >= labelPx) ?? BAR_STEPS[BAR_STEPS.length - 1];
    const first = Math.max(0, Math.floor(fromSec / barSec));
    for (let b = first - (first % skip); b * barSec <= toSecs; b += skip) {
      if (b * barSec < fromSec - barSec) continue;
      bars.push({ sec: b * barSec, bar: b + 1, label: b % labelEvery === 0 });
    }
    if (beatSec * pps >= 7) {
      for (let n = Math.max(0, Math.floor(fromSec / beatSec)); n * beatSec <= toSecs; n++) {
        if (n % 4 !== 0) beats.push(n * beatSec);
      }
    }
  }
  return { time, bars, beats };
}

/** Greedy row assignment so overlapping ranges get different rows. Items are returned with
 * their original index order; `rows` is the number of rows used (at least 1). */
export function packRows(items: { start: number; end: number }[]): { rows: number[]; count: number } {
  const order = items.map((_, i) => i).sort((a, b) => items[a].start - items[b].start || items[a].end - items[b].end);
  const ends: number[] = [];
  const rows = new Array<number>(items.length).fill(0);
  for (const i of order) {
    let r = ends.findIndex((e) => e <= items[i].start);
    if (r < 0) {
      r = ends.length;
      ends.push(0);
    }
    ends[r] = items[i].end;
    rows[i] = r;
  }
  return { rows, count: Math.max(1, ends.length) };
}

/** The part of [start, end] inside [lo, hi], or null. */
export function overlapRange(start: number, end: number, lo: number, hi: number): [number, number] | null {
  const a = Math.max(start, lo);
  const b = Math.min(end, hi);
  return b > a ? [a, b] : null;
}

// ---------------------------------------------------------------------------------------------
// Colours
// ---------------------------------------------------------------------------------------------

/** One colour per clip on the single mix bar, in the order the clips were added (so a colour does
 * not change when a clip is moved). Clearly different hues, bright enough for the dark theme.
 * Hex because the waveform is drawn on a canvas. */
export const CLIP_COLOURS = ["#4aa8ff", "#ff9f43", "#3ddc97", "#ff5d7a", "#b48cff", "#ffd23b", "#2ee6d6", "#ff7ad9"] as const;
/** #rrggbb from hue (degrees), saturation and lightness (0..1). */
export function hslHex(h: number, sat: number, light: number): string {
  const k = (n: number) => (n + h / 30) % 12;
  const a = sat * Math.min(light, 1 - light);
  const f = (n: number) => light - a * Math.max(-1, Math.min(k(n) - 3, Math.min(9 - k(n), 1)));
  return `#${[f(0), f(8), f(4)].map((v) => Math.round(v * 255).toString(16).padStart(2, "0")).join("")}`;
}

/** The first eight clips get the hand-picked colours; after that the hue steps by the golden angle
 * (so neighbours stay far apart however many clips there are) and the lightness cycles, so clip
 * 9 and clip 1 never share a colour. */
export function clipColour(index: number): string {
  const i = Math.max(0, Math.floor(index));
  if (i < CLIP_COLOURS.length) return CLIP_COLOURS[i];
  const n = i - CLIP_COLOURS.length;
  return hslHex((n * 137.508 + 20) % 360, 0.78, [0.62, 0.52, 0.72][Math.floor(n / 8) % 3]);
}

/** `hex` mixed toward white by `amount` (0..1), as #rrggbb. */
export function lighten(hex: string, amount: number): string {
  const n = parseInt(hex.slice(1), 16);
  const ch = (shift: number) => Math.round(((n >> shift) & 255) * (1 - amount) + 255 * amount);
  return `#${[16, 8, 0].map((s) => ch(s).toString(16).padStart(2, "0")).join("")}`;
}

/** The three waveform bands in one clip's colour: lows in the colour itself, mids and highs lighter. */
export const clipBands = (hex: string) => ({ low: hex, mid: lighten(hex, 0.38), high: lighten(hex, 0.72) });

/** Block colours of the tool lane (the mockup's magenta ramp, defined on the view's root in
 * MixesView.svelte; filter in matrix green). */
const TOOL_COLOURS: Record<string, string> = {
  Reverb: "var(--l2c)",
  Echo: "var(--l2b)",
  Roll: "var(--l2e)",
  Wah: "var(--l2d)",
  Vocal: "var(--l2d)",
  Crush: "var(--l2f)",
  FILTER: "var(--green)",
  EQ: "var(--l2a)",
  Volume: "var(--muted)",
  Trans: "var(--warn)",
  loop: "var(--warn)",
  ignore: "var(--muted)",
  reverse: "var(--l2a)",
};
export const toolColour = (name: string) => TOOL_COLOURS[name] ?? "var(--l2a)";

export const TOOL_LABELS: Record<string, string> = { FILTER: "Filter", loop: "Loop", ignore: "Ignore", reverse: "Reverse" };
/** Name of a tool or region. `vocal` is the card's Vocal filter mode (state/vocal.svelte.ts): when it is on, the
 * Wah tool is called "Vocal" (see `toolTypesFor` in state/mixedit.ts). */
export const toolLabel = (name: string, vocal: "off" | "vocal" | "inverted" = "off", marked = false) =>
  name === "Wah" && (marked || vocal !== "off") ? "Vocal" : (TOOL_LABELS[name] ?? name);

// ---------------------------------------------------------------------------------------------
// Crossfade curve presets
// ---------------------------------------------------------------------------------------------

export const CURVE_PRESETS: { name: string; curve: CurveNode[] }[] = [
  { name: "Linear", curve: [{ x: 0, value: 0 }, { x: 1, value: 1 }] },
  { name: "Fast in", curve: [{ x: 0, value: 0 }, { x: 0.25, value: 0.8 }, { x: 1, value: 1 }] },
  { name: "Slow in", curve: [{ x: 0, value: 0 }, { x: 0.75, value: 0.2 }, { x: 1, value: 1 }] },
  { name: "Equal power", curve: [{ x: 0, value: 0 }, { x: 0.25, value: 0.38 }, { x: 0.5, value: 0.71 }, { x: 0.75, value: 0.92 }, { x: 1, value: 1 }] },
  { name: "Cut", curve: [{ x: 0, value: 0 }, { x: 0.5, value: 0 }, { x: 0.52, value: 1 }, { x: 1, value: 1 }] },
];

/** Name of the preset a curve equals (within rounding), or "Custom". */
export function curveName(curve: CurveNode[]): string {
  const same = (a: CurveNode[], b: CurveNode[]) =>
    a.length === b.length && a.every((n, i) => Math.abs(n.x - b[i].x) < 0.005 && Math.abs(n.value - b[i].value) < 0.005);
  return CURVE_PRESETS.find((p) => same(p.curve, curve))?.name ?? "Custom";
}

/** The fader value (0..1) of a crossfade curve at x (0..1). */
export function curveAt(curve: CurveNode[], x: number): number {
  if (curve.length === 0) return x;
  if (x <= curve[0].x) return curve[0].value;
  for (let i = 1; i < curve.length; i++) {
    if (x <= curve[i].x) {
      const a = curve[i - 1];
      const b = curve[i];
      return b.x === a.x ? b.value : a.value + ((b.value - a.value) * (x - a.x)) / (b.x - a.x);
    }
  }
  return curve[curve.length - 1].value;
}

// ---------------------------------------------------------------------------------------------
// Envelope adapters
// ---------------------------------------------------------------------------------------------

/** Key nodes as generic envelope nodes (value in semitones). */
export const keyToEnv = (nodes: KeyNode[]): EnvNode[] => nodes.map((n) => ({ pos: n.pos, value: n.cents / 100 }));

/** Generic nodes back to key nodes; `flags` is kept for nodes that existed, 1 for new ones. */
export function envToKey(nodes: EnvNode[], prev: KeyNode[]): KeyNode[] {
  return nodes.map((n, i) => ({ pos: n.pos, cents: Math.round(n.value * 100), flags: prev.find((p) => p.pos === n.pos)?.flags ?? prev[i]?.flags ?? 1 }));
}

export const roundTo = (x: number, step: number) => (step > 0 ? Math.round(x / step) * step : x);

/** Compact number text for a parameter value. */
export function fmtValue(v: number, unit = ""): string {
  const a = Math.abs(v);
  const t = Number(v.toFixed(a >= 100 ? 0 : a >= 10 ? 1 : 2)).toString();
  return unit ? `${t} ${unit}` : t;
}

/** Signed semitone text: "+2 st", "0 st". */
export const fmtSemis = (st: number) => `${st > 0 ? "+" : st < 0 ? "−" : ""}${Math.abs(Math.round(st * 100) / 100)} st`;

/** Fixed number of columns for a track's waveform: enough for close zoom, capped for memory. */
export function peakColumns(durationSecs: number): number {
  return clamp(Math.ceil(durationSecs * 20), 512, 8192);
}

// ---------------------------------------------------------------------------------------------
// Crossfade curve as an envelope (the node editor works in integer positions)
// ---------------------------------------------------------------------------------------------

export const CURVE_SCALE = 10000;

export const curveToEnv = (curve: CurveNode[]): EnvNode[] => curve.map((n) => ({ pos: Math.round(n.x * CURVE_SCALE), value: n.value }));

export const envToCurve = (nodes: EnvNode[]): CurveNode[] => nodes.map((n) => ({ x: n.pos / CURVE_SCALE, value: n.value }));

// ---------------------------------------------------------------------------------------------
// Overlaps on the one mix bar
// ---------------------------------------------------------------------------------------------

/** How opaque a clip's waveform is where another clip is heard at the same time. */
export const OVERLAP_OPACITY = 0.62;

/** One stretch of a clip's audible part. There are deliberately no lanes: every stretch is drawn
 * the full height of the bar, and where clips overlap they lie over each other, each in its own
 * colour and see-through, as in the original Editor. */
export type ClipSlice = {
  /** Timeline samples. */
  from: number;
  to: number;
  /** Another clip is audible together with this one here. */
  overlapped: boolean;
  /** Stacking order among the clips heard here: 0 is the earliest clip (underneath), the clip that
   * came in last is on top and takes the click. */
  layer: number;
  /** Opacity of the waveform and body: 1 alone, `OVERLAP_OPACITY` when overlapped. */
  opacity: number;
};

/** The audible part of clip `id` cut at every point another clip starts or ends inside it. Clips
 * are ranked by audible start (ties by order in the list). */
export function clipSlices(clips: { id: string; visibleStart: number; visibleEnd: number }[], id: string): ClipSlice[] {
  const me = clips.find((c) => c.id === id);
  if (!me || !(me.visibleEnd > me.visibleStart)) return [];
  const order = clips.map((c, i) => ({ c, i })).sort((a, b) => a.c.visibleStart - b.c.visibleStart || a.i - b.i).map((x) => x.c);
  const others = order.filter((o) => o.id !== id && o.visibleEnd > me.visibleStart && o.visibleStart < me.visibleEnd);
  const cuts = new Set<number>([me.visibleStart, me.visibleEnd]);
  for (const o of others) {
    if (o.visibleStart > me.visibleStart && o.visibleStart < me.visibleEnd) cuts.add(o.visibleStart);
    if (o.visibleEnd > me.visibleStart && o.visibleEnd < me.visibleEnd) cuts.add(o.visibleEnd);
  }
  const xs = [...cuts].sort((a, b) => a - b);
  const out: ClipSlice[] = [];
  for (let i = 0; i + 1 < xs.length; i++) {
    const [from, to] = [xs[i], xs[i + 1]];
    const mid = (from + to) / 2;
    const active = order.filter((o) => o.visibleStart <= mid && o.visibleEnd > mid);
    const overlapped = active.length > 1;
    out.push({ from, to, overlapped, layer: Math.max(0, active.findIndex((o) => o.id === id)), opacity: overlapped ? OVERLAP_OPACITY : 1 });
  }
  return out;
}

/** Where a clip's name goes, in pixels from the top of the bar: at the top, or below the crossfade
 * strip (`stripH` high) when the clip starts inside an overlap and the strip lies over its start. */
export function titleTop(slices: ClipSlice[], stripH: number): number {
  return slices[0]?.overlapped ? stripH + 3 : 3;
}
