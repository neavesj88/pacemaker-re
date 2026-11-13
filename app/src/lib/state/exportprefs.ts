// Pure helpers of the audio export dialog: the loudness and partial-export settings, their
// defaults and limits, clock parsing, and the text for readings. No Svelte, no Tauri (type imports
// only), so they run under plain node (exportprefs.test.mjs). The store that holds the settings is
// state/mixexport.svelte.ts.
import type { AudioFormat, ClipLevel, EncodingSpec, EndAt, FlacLevel, LevelSettings, LoudnessReport, PreviewGains, RangePrefs, RangeSpec, TagSpec, TracklistOptions } from "$lib/api/export";

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

// ---------------------------------------------------------------------------------------------
// formats and tags
// ---------------------------------------------------------------------------------------------

/** The formats, in the order the dialog lists them. `ext` is the file extension the backend
 * requires for the format. */
export const AUDIO_FORMATS: { id: AudioFormat; label: string; ext: string; hint: string }[] = [
  { id: "ogg", label: "Ogg Vorbis", ext: "ogg", hint: "Small lossy files, what the original Editor wrote" },
  { id: "mp3", label: "MP3", ext: "mp3", hint: "Lossy, plays everywhere" },
  { id: "flac", label: "FLAC", ext: "flac", hint: "Lossless, usually half to two thirds the size of a WAV" },
  { id: "wav", label: "WAV (16-bit)", ext: "wav", hint: "Uncompressed" },
];

/** Ogg Vorbis quality is chosen in tenths (the encoder takes -0.1 to 1.0), -1 smallest to 10 best;
 * 6 is what exports have always used. LAME's algorithm quality ("effort") 2 is the one exports
 * have always used; it changes speed and quality, not the bitrate. */
export const OGG_QUALITY_RANGE = [-1, 10] as const;
export const OGG_QUALITY_DEFAULT = 6;
export const MP3_EFFORT_DEFAULT = 2;

export const MP3_CBR_KBPS = [96, 112, 128, 160, 192, 224, 256, 320] as const;

/** LAME's variable-bitrate qualities with the average bitrate its documentation gives for music
 * (approximate: the real rate follows the audio). */
export const MP3_VBR_LEVELS: { quality: number; kbps: number }[] = [245, 225, 190, 175, 165, 130, 115, 100, 85, 65].map((kbps, quality) => ({ quality, kbps }));

export const FLAC_LEVELS: { id: FlacLevel; label: string }[] = [
  { id: "fast", label: "Fast (larger file)" },
  { id: "standard", label: "Standard" },
  { id: "best", label: "Smallest file (slower)" },
];

export const TAG_LIMITS = { text: 250, comment: 2000 } as const;

const FORMAT_EXT = new Map(AUDIO_FORMATS.map((f) => [f.ext, f.id] as const));

/** The format a file name's extension names, or null (case does not matter). */
export function formatOfPath(path: string): AudioFormat | null {
  const m = /\.([A-Za-z0-9]+)$/.exec(path);
  return m ? (FORMAT_EXT.get(m[1].toLowerCase()) ?? null) : null;
}

export const extOf = (f: AudioFormat): string => AUDIO_FORMATS.find((x) => x.id === f)?.ext ?? f;

/** The file the save dialog gave back, and the format it is: a name that ends in a known extension
 * is that format (the user typed or picked it); any other name gets the chosen format's extension. */
export function resolveDest(path: string, chosen: AudioFormat): { path: string; format: AudioFormat } {
  const f = formatOfPath(path);
  return f ? { path, format: f } : { path: `${path.replace(/\.+$/, "")}.${extOf(chosen)}`, format: chosen };
}

/** Whether the export needs to look at what is already on disk before it starts: when the name
 * was changed after the save window closed (its overwrite prompt never saw the final name), or a
 * CUE sheet is written beside the file. */
export const needsDiskCheck = (picked: string, dest: string, writeCue: boolean): boolean => dest !== picked || writeCue;

/** What an export would replace that the save window did not ask about, given what
 * `export_cue_target` found: the lines the confirm lists (empty = nothing to ask), and whether the
 * CUE sheet is replaced. A CUE sheet is replaced without asking when the user's setting says so. */
export function replacements(
  picked: string,
  dest: string,
  format: AudioFormat,
  writeCue: boolean,
  cueIfExists: "replace" | "ask",
  target: { cuePath: string; exists: boolean; audioExists: boolean } | null,
): { items: string[]; replaceCue: boolean } {
  const items: string[] = [];
  let replaceCue = false;
  if (!target) return { items, replaceCue };
  if (dest !== picked && target.audioExists) items.push(`Replace ${dest} (the name got “.${extOf(format)}” added after the save window closed, so it never asked)`);
  if (writeCue && target.exists) {
    replaceCue = true;
    if (cueIfExists !== "replace") items.push(`Replace ${target.cuePath} (the CUE sheet beside it)`);
  }
  return { items, replaceCue };
}

/** The save dialog's file type list with `selected` first (dialogs preselect the first). */
export function saveFilters(selected: AudioFormat): { name: string; extensions: string[] }[] {
  const order = [...AUDIO_FORMATS].sort((a, b) => Number(b.id === selected) - Number(a.id === selected));
  return order.map((f) => ({ name: f.label, extensions: [f.ext] }));
}

/** What the chosen format does with the tags, for the dialog. */
export function tagNote(f: AudioFormat): string {
  switch (f) {
    case "mp3":
      return "MP3 holds all of it: ID3 tags, the cover and the chapters (players without chapter support ignore those).";
    case "flac":
      return "FLAC holds the text and the cover. Chapters are written as CHAPTERnnn fields, which chapter-aware players read.";
    case "ogg":
      return "Ogg Vorbis holds the text and the cover. Chapters are written as CHAPTERnnn fields, which chapter-aware players read.";
    case "wav":
      return "WAV holds the text and chapter marks (cue points), but not a cover image, and many players ignore WAV tags.";
  }
}

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
  // ---- the file ----
  format: AudioFormat;
  mp3Mode: "cbr" | "vbr";
  mp3Kbps: number;
  mp3Quality: number;
  mp3Stereo: "joint" | "stereo";
  mp3Effort: number;
  oggQuality: number;
  flacBits: 16 | 24;
  flacLevel: FlacLevel;
  // ---- the tags ----
  writeTags: boolean;
  /** Written as the artist of the mix. */
  djName: string;
  album: string;
  /** Empty = the current year. */
  year: string;
  comment: string;
  /** A JPEG or PNG file, or empty for no cover. */
  coverPath: string;
  /** Put the tracks of the mix in the file as chapters. */
  chapters: boolean;
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
  format: "ogg",
  mp3Mode: "cbr",
  mp3Kbps: 192,
  mp3Quality: 2,
  mp3Stereo: "joint",
  mp3Effort: MP3_EFFORT_DEFAULT,
  oggQuality: OGG_QUALITY_DEFAULT,
  flacBits: 16,
  flacLevel: "standard",
  writeTags: true,
  djName: "",
  album: "",
  year: "",
  comment: "",
  coverPath: "",
  chapters: true,
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
  if (AUDIO_FORMATS.some((f) => f.id === s.format)) p.format = s.format as AudioFormat;
  if (s.mp3Mode === "cbr" || s.mp3Mode === "vbr") p.mp3Mode = s.mp3Mode;
  if ((MP3_CBR_KBPS as readonly unknown[]).includes(s.mp3Kbps)) p.mp3Kbps = s.mp3Kbps as number;
  if (Number.isInteger(s.mp3Quality) && inRange(s.mp3Quality, [0, 9])) p.mp3Quality = s.mp3Quality as number;
  if (s.mp3Stereo === "joint" || s.mp3Stereo === "stereo") p.mp3Stereo = s.mp3Stereo;
  if (Number.isInteger(s.mp3Effort) && inRange(s.mp3Effort, [0, 9])) p.mp3Effort = s.mp3Effort as number;
  if (Number.isInteger(s.oggQuality) && inRange(s.oggQuality, OGG_QUALITY_RANGE)) p.oggQuality = s.oggQuality as number;
  if (s.flacBits === 16 || s.flacBits === 24) p.flacBits = s.flacBits;
  if (FLAC_LEVELS.some((l) => l.id === s.flacLevel)) p.flacLevel = s.flacLevel as FlacLevel;
  if (typeof s.writeTags === "boolean") p.writeTags = s.writeTags;
  if (typeof s.chapters === "boolean") p.chapters = s.chapters;
  const text = (v: unknown, max: number) => (typeof v === "string" ? v.slice(0, max) : null);
  p.djName = text(s.djName, TAG_LIMITS.text) ?? p.djName;
  p.album = text(s.album, TAG_LIMITS.text) ?? p.album;
  p.year = text(s.year, 4) ?? p.year;
  p.comment = text(s.comment, TAG_LIMITS.comment) ?? p.comment;
  p.coverPath = text(s.coverPath, 1024) ?? p.coverPath;
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

/** The backend's `EncodingSpec` for these preferences. */
export function encodingOf(p: ExportPrefs): EncodingSpec {
  return {
    mp3: p.mp3Mode === "vbr" ? { kind: "vbr", quality: p.mp3Quality } : { kind: "cbr", kbps: p.mp3Kbps },
    mp3Stereo: p.mp3Stereo,
    mp3Effort: p.mp3Effort,
    oggQuality: p.oggQuality,
    flacBits: p.flacBits,
    flacLevel: p.flacLevel,
  };
}

/** The year the tags carry: the one typed, else the current one. */
export const yearOf = (p: ExportPrefs, now: Date = new Date()): string => p.year.trim() || String(now.getFullYear());

/** The backend's `TagSpec`, or null when tags are off. `title` empty means the mix name (the
 * backend fills it). `chapterOptions` are the track list settings, used when chapters are on. */
export function tagsOf(p: ExportPrefs, title: string, chapterOptions: TracklistOptions, now: Date = new Date()): TagSpec | null {
  if (!p.writeTags) return null;
  return {
    title: title.trim(),
    artist: p.djName.trim(),
    album: p.album.trim(),
    year: yearOf(p, now),
    comment: p.comment.trim(),
    coverPath: p.coverPath.trim() || null,
    chapters: p.chapters ? chapterOptions : null,
  };
}

/** About how big the file will be, or null where it depends too much on the music to say: WAV is
 * exact, MP3 follows its bitrate (the average LAME gives for a variable-bitrate quality). */
export function estimateBytes(p: ExportPrefs, durationSecs: number): number | null {
  if (!(durationSecs > 0)) return null;
  if (p.format === "wav") return Math.round(durationSecs * 44100 * 4) + 44;
  if (p.format === "mp3") {
    const kbps = p.mp3Mode === "cbr" ? p.mp3Kbps : (MP3_VBR_LEVELS[p.mp3Quality]?.kbps ?? 0);
    return kbps ? Math.round((kbps * 1000 * durationSecs) / 8) : null;
  }
  return null;
}

/** "12.3 MB" / "840 KB" for a byte count. */
export function sizeText(bytes: number): string {
  return bytes >= 1e6 ? `${(bytes / 1e6).toFixed(bytes >= 1e8 ? 0 : 1)} MB` : `${Math.max(1, Math.round(bytes / 1e3))} KB`;
}

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
    check("The fade at the cuts", p.edgeFadeMs, LIMITS.edgeFadeMs, "ms") ??
    (p.writeTags && p.year.trim() !== "" && !/^\d{4}$/.test(p.year.trim()) ? "The year must be four digits, like 2026, or empty for the current year." : null)
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
