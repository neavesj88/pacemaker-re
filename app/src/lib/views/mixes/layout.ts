// Pure layout helpers of the mix editor's timeline: zoom, ruler ticks, clocks, row packing,
// tool colours, curve presets, envelope adapters. No Svelte, no DOM, no value imports from the
// app (type imports only), so it runs under plain node (layout.test.mjs).
import type { CurveNode, EnvNode, KeyNode } from "$lib/api/mixes";

/** Engine sample rate (same as SR in state/mixedit.ts; duplicated so this file imports no value). */
export const SR = 44100;

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
  const beat = Math.max(0, samples) / ((60 / bpm) * SR);
  return { bar: Math.floor(beat / 4) + 1, beat: Math.floor(beat % 4) + 1 };
}

const TIME_STEPS = [0.5, 1, 2, 5, 10, 15, 30, 60, 120, 300, 600, 1800];
const BAR_STEPS = [1, 2, 4, 8, 16, 32, 64, 128, 256];

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
    const labelEvery = BAR_STEPS.find((n) => n * barPx >= 46) ?? BAR_STEPS[BAR_STEPS.length - 1];
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

/** The three waveform bands (low, mid, high), as in the mockup. Drawn on canvas, so they are
 * constants here rather than CSS variables. */
export const BAND_COLOURS = { low: "#ff48a8", mid: "#e070ff", high: "#00ff46" } as const;

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
