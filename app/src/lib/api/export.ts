// export commands (src-tauri/src/commands/export.rs): mix audio rendering via crates/pm-mixer.
// Engine, algorithms and parameter mappings: notes/mix-audio.md.
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
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
 * editor calls it.
 *
 * `gains`: the `gains` of a loudness report (per-clip levelling and master gain): the preview then
 * plays at the level an export with the same settings has. `edgeFadeMs`: the fade at both ends of
 * the window, to hear a range as a partial export makes it (omit for plain playback). */
export const renderMixPreview = (root: string, mix: MixDoc, fromSec: number, toSec: number, gains?: PreviewGains, edgeFadeMs?: number) =>
  invoke<RenderedMix>("export_render_preview", { root, mix, fromSec, toSec, gains: gains ?? null, edgeFadeMs: edgeFadeMs ?? null });

/** What `exportMixAudio` returns the moment the job has started. */
export type MixExportStarted = {
  path: string;
  /** The CUE sheet that will be written beside the audio, when one was asked for. */
  cuePath: string | null;
  /** A file was already at `cuePath` and will be replaced (only when `replaceCue` said it may). */
  cueReplaces: boolean;
};

/** Where the CUE sheet of an audio export to `dest` would go, and whether a file is already there
 * (`audioExists`: also whether `dest` itself exists). */
export type CueTarget = { cuePath: string; exists: boolean; /** A file is already at `dest` itself. */ audioExists: boolean };

/** Asks before an export: the CUE sheet it would write beside `dest`, and whether one exists.
 * Reads nothing but the file's existence. */
export const exportCueTarget = (dest: string) => invoke<CueTarget>("export_cue_target", { dest });

/** Whether a cover image can still be used for the tags: null when it can, else the sentence the
 * export would refuse with (the same check the export runs). Changes nothing. */
export const exportCoverCheck = (path: string) => invoke<string | null>("export_cover_check", { path });

/** The `job` name of mix exports in the `job-progress` / `job-done` events. */
export const MIX_EXPORT_JOB = "mix-export";

/** The audio formats an export can write. */
export type AudioFormat = "ogg" | "wav" | "flac" | "mp3";

/** How an MP3 is encoded: a constant bitrate (kbit/s, one of 96 112 128 160 192 224 256 320) or
 * LAME's variable bitrate at a quality from 0 (best) to 9 (smallest). */
export type Mp3Spec = { kind: "cbr"; kbps: number } | { kind: "vbr"; quality: number };

/** How hard the FLAC encoder works for a smaller file; the audio is identical at every level. */
export type FlacLevel = "fast" | "standard" | "best";

/** The settings of the formats that have any; the ones of the format not being written are
 * ignored. `flacBits` is 16 (dithered, like the WAV) or 24. */
export type EncodingSpec = {
  mp3: Mp3Spec;
  /** How the two channels are coded: "joint" shares what they have in common, "stereo" codes them separately. */
  mp3Stereo: "joint" | "stereo";
  /** LAME's algorithm quality, 0 (best, slowest) to 9 (fastest); the bitrate does not change. */
  mp3Effort: number;
  /** Ogg Vorbis quality in tenths, -1 (smallest) to 10 (best). */
  oggQuality: number;
  flacBits: 16 | 24;
  flacLevel: FlacLevel;
};

/** The tags to write into the exported file (never into the device). Empty text is not written;
 * an empty `title` becomes the mix name. `coverPath` is a JPEG or PNG file up to 8 MB (read and
 * checked before the export starts). `chapters`: the track list settings to make the tracks of the
 * mix chapters of the file (from the same list the CUE sheet uses); null = no chapters. What each
 * format can hold: MP3 everything (ID3v2.3, CHAP frames), FLAC and Ogg text, cover and CHAPTERnnn
 * fields, WAV text and cue points but no cover. */
export type TagSpec = {
  title: string;
  artist: string;
  album: string;
  year: string;
  comment: string;
  coverPath: string | null;
  chapters: TracklistOptions | null;
};

/** Render a whole mix to an audio file (format "ogg" like the original Editor, "wav", "flac" or "mp3").
 * This starts a background job and resolves AT ONCE with the destination; progress arrives as
 * `job-progress` events (job "mix-export", `done` / `total` in seconds of audio) and the end as
 * `job-done` (its `summary` is the finished line, `failed` counts clips left out). The file
 * exists under `path` only after `job-done`. Rejects when another job is running, the format is
 * unknown, `dest` does not end in the format's extension (.ogg / .wav / .flac / .mp3), or `dest` is inside the device volume or in a folder that does not exist. Rendering
 * starts at the mix's first audible frame.
 *
 * `cue`: pass the track list options to also write a CUE sheet beside the audio (same name,
 * `.cue`) when the render has finished. An existing `.cue` of that name is replaced only when
 * `replaceCue` is true (`cueReplaces` in the result says it happened); otherwise the call rejects
 * and nothing starts. Rejects up front when the mix cannot have one (more than 99 tracks). Omit
 * `cue` for audio only.
 *
 * `extras.range` exports only a part of the mix (see `RangeSpec`; omitted = the whole mix) and
 * `extras.levels` is the loudness setting (omitted = none): with a target or clip levelling the
 * job measures first (progress status "measuring", the first half of the bar), then renders with
 * the gains found. A CUE sheet of a partial export counts from the start of the range. When the
 * file is written, a `mix-loudness-report` event carries what it measures (`onLoudnessReport`),
 * and the `job-done` summary ends with that loudness.
 *
 * `extras.encoding` is the format's settings (MP3 bitrate, FLAC sample size) and `extras.tags` the
 * tags, cover and chapters (see `TagSpec`); without tags Ogg gets the mix name as its title and the
 * other formats none. A cover that cannot be read, or a setting out of range, rejects the call
 * before anything starts. */
export const exportMixAudio = (root: string, mix: MixDoc, dest: string, format: AudioFormat, cue?: TracklistOptions, replaceCue = false, extras: ExportExtras = {}) =>
  invoke<MixExportStarted>("export_mix_audio", {
    root,
    mix,
    dest,
    format,
    cue: cue ?? null,
    replaceCue,
    range: extras.range ?? null,
    rangePrefs: extras.rangePrefs ?? null,
    levels: extras.levels ?? null,
    encoding: extras.encoding ?? null,
    tags: extras.tags ?? null,
  });

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

// ---------------------------------------------------------------------------------------------
// Partial export and loudness (src-tauri/src/commands/export/{range,loudness}.rs)
// ---------------------------------------------------------------------------------------------

/** Which part of the mix an export covers. `time`: timeline seconds, as the ruler shows them (the
 * start is kept exactly; the end is cut to the last clip). `clips`: from the first audible frame
 * of one clip to the end of another, given by clip id in either order. */
export type RangeSpec =
  | { kind: "whole" }
  | { kind: "time"; fromSec: number; toSec: number }
  | { kind: "clips"; fromClip: string; toClip: string };

/** Where a "clip A to clip B" range ends: with the clip's last audible frame (`clipEnd`; the next
 * track fading in under its tail is cut there) or just before the next clip enters (`nextEnters`). */
export type EndAt = "clipEnd" | "nextEnters";

/** The user's settings for partial exports. */
export type RangePrefs = {
  endAt: EndAt;
  /** Fade at a cut that falls inside audio, in milliseconds; 0 = a hard cut. */
  edgeFadeMs: number;
};

/** What an export of a range covers; the numbers the export itself uses. */
export type RangeInfo = {
  /** The request is the whole mix. */
  whole: boolean;
  fromSecs: number;
  toSecs: number;
  durationSecs: number;
  /** The clips heard in the range, in timeline order. */
  clipIds: string[];
};

/** What an export of `range` covers, computed by the same code the export runs (also for the
 * whole mix). Rejects with a sentence for the user when the range is empty, silent, or names a
 * clip that is not heard. Read-only and quick. */
export const exportRangeResolve = (mix: MixDoc, range: RangeSpec, rangePrefs?: RangePrefs) =>
  invoke<RangeInfo>("export_range_resolve", { mix, range, rangePrefs: rangePrefs ?? null });

/** How clips are levelled against each other: not at all, to the median loudness of the clips in
 * the export, or to a fixed level. */
export type ClipLevel = "off" | "median" | "fixed";

/** The loudness settings of an export (and of the preview that must match it). */
export type LevelSettings = {
  /** Integrated loudness to reach in LUFS; null = no target. */
  targetLufs: number | null;
  /** The most the master gain may raise or lower the mix, dB. */
  maxBoostDb: number;
  maxCutDb: number;
  /** Never raise the mix past `peakCeilingDb` (true peak) instead of letting the limiter work. */
  capAtPeak: boolean;
  peakCeilingDb: number;
  clipLevel: ClipLevel;
  /** Reference for `clipLevel: "fixed"`, LUFS. */
  clipFixedLufs: number;
  /** The most a clip may be raised or lowered by levelling, dB. */
  clipMaxDb: number;
};

/** A loudness reading; null where the value is not defined (silence). */
export type Reading = { lufs: number | null; truePeakDb: number | null; samplePeakDb: number | null };

export type ClipLoudness = {
  clipId: string;
  /** The clip's own integrated loudness; null when too short, silent, or its audio is missing. */
  lufs: number | null;
  truePeakDb: number | null;
  /** The gain levelling applies to it, dB. */
  gainDb: number;
  note: string | null;
};

/** Gains for `renderMixPreview`: dB per clip id (a clip not listed has 0) and the master gain. */
export type PreviewGains = { clipGainDb: Record<string, number>; masterGainDb: number };

/** What a loudness analysis (or the end of an export) found. */
export type LoudnessReport = {
  /** "analysis" or "export". */
  source: "analysis" | "export";
  /** The part of the timeline measured, seconds. */
  fromSecs: number;
  toSecs: number;
  /** Levelling table (empty when clip levelling is off) and the level the clips were moved to. */
  clips: ClipLoudness[];
  clipReferenceLufs: number | null;
  /** The mix entering the master limiter, with the clip gains and no master gain. */
  beforeLimiter: Reading | null;
  /** The same after the limiter. */
  afterLimiter: Reading | null;
  targetLufs: number | null;
  /** The master gain the export applies, dB. */
  masterGainDb: number;
  /** The true peak entering the limiter with the master gain (predicted), dBTP. */
  predictedPeakDb: number | null;
  /** With the master gain the peaks pass the limiter's ceiling: the target will be missed. */
  limiterWillAct: boolean;
  /** The finished file, measured (export reports only). */
  result: Reading | null;
  notes: string[];
  /** What to give `renderMixPreview` to hear the export's level. */
  gains: PreviewGains;
};

/** Optional parts of `exportMixAudio`. */
export type ExportExtras = { range?: RangeSpec; rangePrefs?: RangePrefs; levels?: LevelSettings; encoding?: EncodingSpec; tags?: TagSpec | null };

/** The `job` name of loudness analyses in `job-progress` / `job-done`. */
export const MIX_LOUDNESS_JOB = "mix-loudness";

/** Starts a loudness analysis and resolves at once. It writes no file: with clip levelling on it
 * measures every clip heard in the range alone, then renders the mix with those gains and
 * measures it before and after the limiter. Progress is `job-progress` (job "mix-loudness",
 * status "measuring"), the result one `mix-loudness-report` event (`onLoudnessReport`, source
 * "analysis") followed by `job-done`. Rejects when another job is running or the settings or range
 * are invalid. `exportCancel` stops it. */
export const exportLoudnessAnalyse = (root: string, mix: MixDoc, range?: RangeSpec, rangePrefs?: RangePrefs, levels?: LevelSettings) =>
  invoke<void>("export_loudness_analyse", { root, mix, range: range ?? null, rangePrefs: rangePrefs ?? null, levels: levels ?? null });

/** Listens for loudness reports (an analysis finishing, or an export measuring its file). */
export const onLoudnessReport = (cb: (r: LoudnessReport) => void) => listen<LoudnessReport>("mix-loudness-report", (e) => cb(e.payload));
