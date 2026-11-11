// Pure helpers of the track list settings (no Svelte, no Tauri), so they can be tested with plain
// node (tracklistprefs.test.mjs). The store that holds them is state/tracklist.svelte.ts.
import type { CloseEntries, Rounding, StampAt, TracklistFormat, TracklistOptions } from "$lib/api/export";

/** The defaults. They are the backend's defaults too (TracklistOptions::default in tracklist.rs):
 * the backend fills any field the UI leaves out with the same values. */
export const DEFAULT_OPTIONS: TracklistOptions = {
  stampAt: "enters",
  rounding: "floor",
  closeEntries: "merge",
  includeUnavailable: false,
  numbered: true,
  csvBom: true,
};

export type TracklistPrefs = {
  options: TracklistOptions;
  /** Write a CUE sheet beside every audio export. */
  writeCue: boolean;
  /** The format tab last used in the dialog. */
  format: TracklistFormat;
};

export const FORMATS: { id: TracklistFormat; label: string; ext: "txt" | "csv" | "cue"; hint: string }[] = [
  { id: "txt", label: "Text", ext: "txt", hint: "One line per track with its start time" },
  { id: "csv", label: "CSV", ext: "csv", hint: "A spreadsheet: number, start, seconds, artist, title, album, key, BPM" },
  { id: "youtube", label: "YouTube chapters", ext: "txt", hint: "For a video description: first at 0:00, three or more, each at least 10 seconds" },
  { id: "mixcloud", label: "Mixcloud list", ext: "txt", hint: "Numbered Artist - Title (start) lines to copy from" },
  { id: "cue", label: "CUE sheet", ext: "cue", hint: "For the rendered audio file: players show the tracks as chapters" },
];

const STAMPS: StampAt[] = ["enters", "crossfadeMid"];
const ROUNDINGS: Rounding[] = ["floor", "nearest"];
const CLOSES: CloseEntries[] = ["merge", "drop"];

export function defaultPrefs(): TracklistPrefs {
  return { options: { ...DEFAULT_OPTIONS }, writeCue: false, format: "txt" };
}

/** Settings read back from storage: anything missing or of the wrong type falls back to the default. */
export function mergePrefs(saved: unknown): TracklistPrefs {
  const prefs = defaultPrefs();
  if (!saved || typeof saved !== "object") return prefs;
  const s = saved as Record<string, unknown>;
  if (typeof s.writeCue === "boolean") prefs.writeCue = s.writeCue;
  if (FORMATS.some((f) => f.id === s.format)) prefs.format = s.format as TracklistFormat;
  const o = (s.options && typeof s.options === "object" ? s.options : {}) as Record<string, unknown>;
  if (STAMPS.includes(o.stampAt as StampAt)) prefs.options.stampAt = o.stampAt as StampAt;
  if (ROUNDINGS.includes(o.rounding as Rounding)) prefs.options.rounding = o.rounding as Rounding;
  if (CLOSES.includes(o.closeEntries as CloseEntries)) prefs.options.closeEntries = o.closeEntries as CloseEntries;
  for (const k of ["includeUnavailable", "numbered", "csvBom"] as const) if (typeof o[k] === "boolean") prefs.options[k] = o[k];
  return prefs;
}

/** A file name for the rendered audio of a mix, safe on Windows (no reserved characters, no
 * trailing dot or space), with the extension of `format`. */
export function audioFileName(mixName: string, format: "ogg" | "wav" = "ogg"): string {
  // eslint-disable-next-line no-control-regex
  const base = mixName.replace(/[\u0000-\u001f<>:"/\\|?*]/g, " ").replace(/\s+/g, " ").trim().replace(/[. ]+$/, "");
  return `${base || "mix"}.${format}`;
}

/** `name` with `ext` as its extension: appended when missing (case-insensitive), so a save dialog
 * that returned a bare name still gives a file the backend accepts. */
export function withExtension(name: string, ext: string): string {
  return name.toLowerCase().endsWith("." + ext.toLowerCase()) ? name : `${name}.${ext}`;
}

/** The file name only (either path style). */
export function baseName(path: string): string {
  return path.split(/[\\/]/).pop() ?? path;
}

/** The CUE sheet that goes with an audio file: the same path with the extension replaced. */
export function cuePathFor(audioPath: string): string {
  return audioPath.replace(/\.[^.\\/]*$/, "") + ".cue";
}

/** The default name of a saved list file: `<mix name> - <format label>.<ext>`; a CUE sheet is
 * named like the audio so a player finds the pair. */
export function listFileName(mixName: string, format: TracklistFormat): string {
  const f = FORMATS.find((x) => x.id === format)!;
  const stem = audioFileName(mixName, "ogg").replace(/\.ogg$/, "");
  return format === "cue" ? `${stem}.cue` : `${stem} - ${f.label}.${f.ext}`;
}
