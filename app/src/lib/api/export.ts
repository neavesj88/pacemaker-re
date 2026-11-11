// export commands (src-tauri/src/commands/export.rs): mix audio rendering via crates/pm-mixer.
// Engine, algorithms and parameter mappings: notes/mix-audio.md.
import { invoke } from "@tauri-apps/api/core";
import type { MixDoc } from "./mixes";


/** A finished render. `path` is the contract; the rest is extra information. */
export type RenderedMix = {
  path: string;
  /** Length of the rendered audio in seconds. */
  durationSecs: number;
  /** Clips that could not be rendered (removed track, missing file) and were left out. */
  warnings: string[];
  /** How many clips were left out. */
  clipsFailed: number;
};

/** Render part of a mix to a temporary WAV for preview playback; returns its path (play it with
 * playback.previewMix). `mix` is the editor's document (api/mixes.ts), saved or not; `fromSec` and
 * `toSec` are MIX TIMELINE seconds (the file starts at `fromSec`). At most 15 minutes per call.
 * The file is one of four rotating temp files, so a preview stays valid for the next three
 * renders. Cross-area contract: the mix audio area implements `export_render_preview`; the mix
 * editor calls it. */
export const renderMixPreview = (root: string, mix: MixDoc, fromSec: number, toSec: number) =>
  invoke<RenderedMix>("export_render_preview", { root, mix, fromSec, toSec });

/** What `exportMixAudio` returns the moment the job has started. */
export type MixExportStarted = {
  path: string;
  /** The CUE sheet that will be written beside the audio, when one was asked for. */
  cuePath: string | null;
  /** A file was already at `cuePath` and will be replaced (only when `replaceCue` said it may). */
  cueReplaces: boolean;
};

/** Where the CUE sheet of an audio export to `dest` would go, and whether a file is already there. */
export type CueTarget = { cuePath: string; exists: boolean };

/** Asks before an export: the CUE sheet it would write beside `dest`, and whether one exists.
 * Reads nothing but the file's existence. */
export const exportCueTarget = (dest: string) => invoke<CueTarget>("export_cue_target", { dest });

/** The `job` name of mix exports in the `job-progress` / `job-done` events. */
export const MIX_EXPORT_JOB = "mix-export";

/** Render a whole mix to an audio file (format "ogg" like the original Editor, or "wav").
 * This starts a background job and resolves AT ONCE with the destination; progress arrives as
 * `job-progress` events (job "mix-export", `done` / `total` in seconds of audio) and the end as
 * `job-done` (its `summary` is the finished line, `failed` counts clips left out). The file
 * exists under `path` only after `job-done`. Rejects when another job is running, the format is
 * unknown, `dest` does not end in the format's extension (.ogg / .wav), or `dest` is inside the device volume or in a folder that does not exist. Rendering
 * starts at the mix's first audible frame.
 *
 * `cue`: pass the track list options to also write a CUE sheet beside the audio (same name,
 * `.cue`) when the render has finished. An existing `.cue` of that name is replaced only when
 * `replaceCue` is true (`cueReplaces` in the result says it happened); otherwise the call rejects
 * and nothing starts. Rejects up front when the mix cannot have one (more than 99 tracks). Omit
 * `cue` for audio only. */
export const exportMixAudio = (root: string, mix: MixDoc, dest: string, format: "ogg" | "wav", cue?: TracklistOptions, replaceCue = false) =>
  invoke<MixExportStarted>("export_mix_audio", { root, mix, dest, format, cue: cue ?? null, replaceCue });

/** Asks the running mix export to stop. The render ends at its next block, no file is left, and
 * the job reports `job-done` with a "cancelled" summary (not an error). Does nothing when no
 * export is running. */
export const exportCancel = () => invoke<void>("export_cancel");

// ---------------------------------------------------------------------------------------------
// Track list (src-tauri/src/commands/export/tracklist.rs)
// ---------------------------------------------------------------------------------------------

/** Which moment of a clip's entrance the list calls its start: "enters" = the first audible frame
 * (where its crossfade begins), "crossfadeMid" = the middle of the overlap with the clips already
 * playing. */
export type StampAt = "enters" | "crossfadeMid";
/** How the text formats cut a time to whole seconds. A CUE sheet is exact and ignores this. */
export type Rounding = "floor" | "nearest";
/** What the YouTube list does with a track less than 10 s after the previous chapter. */
export type CloseEntries = "merge" | "drop";

/** Every policy choice of the track list (the user's settings; the backend fills defaults for
 * anything missing). */
export type TracklistOptions = {
  stampAt: StampAt;
  rounding: Rounding;
  closeEntries: CloseEntries;
  /** List a clip whose audio is not on the device (it is silent in the export). */
  includeUnavailable: boolean;
  /** Number the lines of the plain list. */
  numbered: boolean;
  /** Start the CSV file with a UTF-8 byte-order mark (Excel needs it for non-ASCII names). */
  csvBom: boolean;
};

export type TracklistFormat = "txt" | "csv" | "youtube" | "mixcloud" | "cue";

export type TracklistEntry = {
  /** 1-based position in the list. */
  number: number;
  clipId: string;
  trackId: number;
  artist: string;
  title: string;
  album: string;
  key: string;
  bpm: number;
  /** The clip's audio is on the device (false only when `includeUnavailable` listed it anyway). */
  available: boolean;
  /** Start in frames (44.1 kHz) from the start of the exported audio file. */
  startFrames: number;
  startSecs: number;
  /** The start as the plain list prints it. */
  startLabel: string;
  /** The clip begins while an earlier listed clip is still playing. */
  overlapsPrevious: boolean;
};

/** One list in every format. The texts are exactly what `exportTracklistSave` writes (a CSV or CUE
 * file may add a byte-order mark). */
export type TracklistOut = {
  entries: TracklistEntry[];
  /** Length of the exported audio file. */
  durationSecs: number;
  txt: string;
  csv: string;
  youtube: string;
  /** YouTube will take `youtube` as chapters (first 0:00, three or more, each 10 s or longer). */
  youtubeValid: boolean;
  /** What was changed to meet YouTube's rules, and what could not be met. */
  youtubeIssues: string[];
  mixcloud: string;
  /** Empty when `cueIssue` is set. */
  cue: string;
  cueIssue: string | null;
  /** Clips left out of the list, and why. */
  warnings: string[];
};

/** The track list of a mix: the editor's document (saved or not), in every format. Times are the
 * times in the exported audio file (its first audible frame is 0:00), computed by the same code the
 * renderer uses. `audioFileName` is the file the CUE sheet points at (a folder part is ignored). */
export const exportTracklist = (root: string, mix: MixDoc, options: TracklistOptions, audioFileName: string) =>
  invoke<TracklistOut>("export_tracklist", { root, mix, options, audioFileName });

/** Writes one format of the list to `dest` (must end in .txt, .csv or .cue to match the format and
 * be outside the device volume; an existing file is replaced) and returns the path. Same text as
 * `exportTracklist` shows for the same arguments. */
export const exportTracklistSave = (
  root: string,
  mix: MixDoc,
  options: TracklistOptions,
  format: TracklistFormat,
  dest: string,
  audioFileName: string,
) => invoke<string>("export_tracklist_save", { root, mix, options, format, dest, audioFileName });
