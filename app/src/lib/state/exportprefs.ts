// Pure helpers of the audio export dialog: the loudness and partial-export settings, their
// defaults and limits, clock parsing, and the text for readings. No Svelte, no Tauri (type imports
// only), so they run under plain node (exportprefs.test.mjs). The store that holds the settings is
// state/mixexport.svelte.ts.
import type { ClipLevel, EndAt, LevelSettings, LoudnessReport, PreviewGains, RangePrefs, RangeSpec } from "$lib/api/export";

export type TargetPreset = "off" | "streaming" | "club" | "custom";

/** The target loudness choices. The numbers are the ones DJs and platforms quote: about -14 LUFS
 * is what streaming services normalise to, about -9 LUFS is a loud club master. The user can
 * enter any value with "custom". */
export const TARGET_PRESETS: { id: TargetPreset; label: string; lufs: number | null; hint: string }[] = [
  { id: "off", label: "Off", lufs: null, hint: "Keep the level the mix has" },
  { id: "streaming", label: "Streaming (-14 LUFS)", lufs: -14, hint: "About what streaming services play at" },
  { id: "club", label: "Club (-9 LUFS)", lufs: -9, hint: "Loud; the limiter works hard unless the tracks are already loud" },
  { id: "custom", label: "Custom", lufs: null, hint: "Any value between -60 and 0 LUFS" },
];

export type RangeMode = "whole" | "time" | "clips";

/** Limits the backend enforces (LevelSettings::validate, RangePrefs); the dialog checks the same. */
export const LIMITS = {
  target: [-60, 0],
  maxBoost: [0, 40],
  maxCut: [0, 60],
  peakCeiling: [-20, 0],
  clipFixed: [-60, 0],
  clipMax: [0, 30],
  edgeFadeMs: [0, 2000],
} as const;

/** Everything the user can set. The defaults do nothing to the audio: no target, no levelling. */
export type ExportPrefs = {
  targetPreset: TargetPreset;
  /** The value of the "custom" target, LUFS. */
  customTarget: number;
  maxBoostDb: number;
  maxCutDb: number;
  capAtPeak: boolean;
  peakCeilingDb: number;
  clipLevel: ClipLevel;
  clipFixedLufs: number;
  clipMaxDb: number;
  endAt: EndAt;
  edgeFadeMs: number;
  /** Play the editor's preview at the level an export with these settings would have (needs a
   * current analysis). */
  previewAtExportLevel: boolean;
};

/** The defaults. They match the backend's (LevelSettings::default, RangePrefs::default). */
export const DEFAULT_PREFS: ExportPrefs = {
  targetPreset: "off",
  customTarget: -14,
  maxBoostDb: 12,
  maxCutDb: 24,
  capAtPeak: false,
  peakCeilingDb: -1,
  clipLevel: "off",
  clipFixedLufs: -14,
  clipMaxDb: 9,
  endAt: "clipEnd",
  edgeFadeMs: 20,
  previewAtExportLevel: true,
};

export const defaultPrefs = (): ExportPrefs => ({ ...DEFAULT_PREFS });

const inRange = (v: unknown, [lo, hi]: readonly [number, number]): v is number => typeof v === "number" && Number.isFinite(v) && v >= lo && v <= hi;

/** Settings read back from storage: anything missing, of the wrong type or out of range falls
 * back to its default. */
export function mergePrefs(saved: unknown): ExportPrefs {
  const p = defaultPrefs();
  if (!saved || typeof saved !== "object") return p;
  const s = saved as Record<string, unknown>;
  if (TARGET_PRESETS.some((t) => t.id === s.targetPreset)) p.targetPreset = s.targetPreset as TargetPreset;
  if (inRange(s.customTarget, LIMITS.target)) p.customTarget = s.customTarget;
  if (inRange(s.maxBoostDb, LIMITS.maxBoost)) p.maxBoostDb = s.maxBoostDb;
  if (inRange(s.maxCutDb, LIMITS.maxCut)) p.maxCutDb = s.maxCutDb;
  if (typeof s.capAtPeak === "boolean") p.capAtPeak = s.capAtPeak;
  if (inRange(s.peakCeilingDb, LIMITS.peakCeiling)) p.peakCeilingDb = s.peakCeilingDb;
  if (s.clipLevel === "off" || s.clipLevel === "median" || s.clipLevel === "fixed") p.clipLevel = s.clipLevel;
  if (inRange(s.clipFixedLufs, LIMITS.clipFixed)) p.clipFixedLufs = s.clipFixedLufs;
  if (inRange(s.clipMaxDb, LIMITS.clipMax)) p.clipMaxDb = s.clipMaxDb;
  if (s.endAt === "clipEnd" || s.endAt === "nextEnters") p.endAt = s.endAt;
  if (inRange(s.edgeFadeMs, LIMITS.edgeFadeMs)) p.edgeFadeMs = s.edgeFadeMs;
  if (typeof s.previewAtExportLevel === "boolean") p.previewAtExportLevel = s.previewAtExportLevel;
  return p;
}

/** The target in LUFS the settings choose, or null for none. */
export function targetOf(p: ExportPrefs): number | null {
  if (p.targetPreset === "custom") return p.customTarget;
  return TARGET_PRESETS.find((t) => t.id === p.targetPreset)?.lufs ?? null;
}

/** The backend's `LevelSettings` for these preferences. */
export function levelsOf(p: ExportPrefs): LevelSettings {
  return {
    targetLufs: targetOf(p),
    maxBoostDb: p.maxBoostDb,
    maxCutDb: p.maxCutDb,
    capAtPeak: p.capAtPeak,
    peakCeilingDb: p.peakCeilingDb,
    clipLevel: p.clipLevel,
    clipFixedLufs: p.clipFixedLufs,
    clipMaxDb: p.clipMaxDb,
  };
}

export const rangePrefsOf = (p: ExportPrefs): RangePrefs => ({ endAt: p.endAt, edgeFadeMs: p.edgeFadeMs });

/** Does the setting change the audio at all (a target or clip levelling)? */
export const levelsActive = (l: LevelSettings): boolean => l.targetLufs !== null || l.clipLevel !== "off";

/** A sentence about the first setting that is out of range, or null. Mirrors the backend's check,
 * so a bad value is caught before anything starts. */
export function problemWith(p: ExportPrefs): string | null {
  const check = (name: string, v: number, lim: readonly [number, number], unit: string) =>
    typeof v === "number" && Number.isFinite(v) && v >= lim[0] && v <= lim[1] ? null : `${name} must be between ${lim[0]} and ${lim[1]} ${unit}.`;
  const target = p.targetPreset === "custom" ? check("The target loudness", p.customTarget, LIMITS.target, "LUFS") : null;
  return (
    target ??
    check("The maximum boost", p.maxBoostDb, LIMITS.maxBoost, "dB") ??
    check("The maximum cut", p.maxCutDb, LIMITS.maxCut, "dB") ??
    (p.capAtPeak ? check("The true-peak ceiling", p.peakCeilingDb, LIMITS.peakCeiling, "dBTP") : null) ??
    (p.clipLevel === "fixed" ? check("The clip level", p.clipFixedLufs, LIMITS.clipFixed, "LUFS") : null) ??
    (p.clipLevel !== "off" ? check("The maximum clip gain", p.clipMaxDb, LIMITS.clipMax, "dB") : null) ??
    check("The fade at the cuts", p.edgeFadeMs, LIMITS.edgeFadeMs, "ms")
  );
}

// ---------------------------------------------------------------------------------------------
// clock text
// ---------------------------------------------------------------------------------------------

/** Seconds from "83", "1:23", "1:23.5" or "1:02:03" (a comma works as the decimal point); null if it
 * is not a time. */
export function parseClock(text: string): number | null {
  const t = text.trim().replace(",", ".");
  if (!/^\d+(:\d{1,2}){0,2}(\.\d+)?$/.test(t)) return null;
  const parts = t.split(":").map(Number);
  if (parts.length > 1 && parts.slice(1).some((x) => x >= 60)) return null;
  return parts.reduce((acc, x) => acc * 60 + x, 0);
}

/** "m:ss.t" (or "h:mm:ss.t" from an hour) for a time in seconds, the form `parseClock` reads. */
export function clockText(sec: number): string {
  const s = Math.max(0, sec);
  const tenths = Math.round(s * 10);
  const whole = Math.floor(tenths / 10);
  const h = Math.floor(whole / 3600);
  const m = Math.floor(whole / 60) % 60;
  const r = whole % 60;
  const frac = tenths % 10;
  const sec2 = String(r).padStart(2, "0");
  return h > 0 ? `${h}:${String(m).padStart(2, "0")}:${sec2}.${frac}` : `${m}:${sec2}.${frac}`;
}

/** "m:ss" for a duration, without tenths (h:mm:ss from an hour). */
export function durationText(sec: number): string {
  const s = Math.max(0, Math.round(sec));
  const h = Math.floor(s / 3600);
  const m = Math.floor(s / 60) % 60;
  return h > 0 ? `${h}:${String(m).padStart(2, "0")}:${String(s % 60).padStart(2, "0")}` : `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
}

// ---------------------------------------------------------------------------------------------
// the request and the analysis
// ---------------------------------------------------------------------------------------------

/** The backend's `RangeSpec` for the dialog's choice. `time` text that does not parse gives null. */
export function rangeSpecOf(mode: RangeMode, from: number | null, to: number | null, clipA: string, clipB: string): RangeSpec | null {
  if (mode === "whole") return { kind: "whole" };
  if (mode === "time") return from === null || to === null ? null : { kind: "time", fromSec: from, toSec: to };
  return clipA && clipB ? { kind: "clips", fromClip: clipA, toClip: clipB } : null;
}

/** Identifies the question an analysis answered. An analysis is current only for the same key and
 * the same mix audio: change a setting or the range and it is stale. */
export function analysisKey(levels: LevelSettings, range: RangeSpec, rp: RangePrefs): string {
  return JSON.stringify([levels, range, rp.endAt, rp.edgeFadeMs]);
}

/** The gains a preview should play with, or undefined for none. */
export function previewGainsOf(report: LoudnessReport | null, current: boolean, on: boolean): PreviewGains | undefined {
  if (!on || !report || !current) return undefined;
  const g = report.gains;
  const flat = g.masterGainDb === 0 && Object.values(g.clipGainDb).every((v) => v === 0);
  return flat ? undefined : g;
}

// ---------------------------------------------------------------------------------------------
// text
// ---------------------------------------------------------------------------------------------

/** "-14.2 LUFS", or "none" where silence has no loudness. */
export const lufsText = (v: number | null | undefined): string => (v === null || v === undefined ? "none" : `${v.toFixed(1)} LUFS`);
/** "-1.0 dBTP" (true peak) or "none" for digital silence. */
export const peakText = (v: number | null | undefined): string => (v === null || v === undefined ? "none" : `${v.toFixed(1)} dBTP`);
/** "+3.2 dB" / "-1.0 dB" / "0.0 dB". */
export const gainText = (db: number): string => (Math.abs(db) < 0.05 ? "0.0 dB" : `${db > 0 ? "+" : ""}${db.toFixed(1)} dB`);
