// Import commands (src-tauri/src/commands/import.rs) and the OS-level file pick / drop.
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open, save } from "@tauri-apps/plugin-dialog";

export const AUDIO_EXTS = ["mp3", "flac", "wav", "m4a", "aac", "ogg", "aiff"];

/** Files the user picks in the native dialog (empty if cancelled). */
export async function pickAudioFiles(): Promise<string[]> {
  const sel = await open({
    multiple: true,
    title: "Choose audio files to add",
    filters: [{ name: "Audio", extensions: AUDIO_EXTS }],
  });
  return Array.isArray(sel) ? sel : typeof sel === "string" ? [sel] : [];
}

/** The audio files among these paths; folders are searched recursively. */
export const expandAudioPaths = (paths: string[]) => invoke<string[]>("expand_audio_paths", { paths });

/** Analyse and add files to the library on `root`. Returns when the job has started. */
export const addTracks = (root: string, files: string[]) => invoke<void>("add_tracks", { root, files });

export type DragState = { type: "hover" } | { type: "leave" } | { type: "drop"; paths: string[] };

/** OS file drops arrive as Tauri webview events (not HTML5 drag events); absent in a plain
 *  browser, where the returned promise rejects (use it with `subscribeAll`). */
export const onFileDrag = (cb: (s: DragState) => void) =>
  getCurrentWebview().onDragDropEvent((e) => {
    const t = e.payload.type;
    if (t === "enter" || t === "over") cb({ type: "hover" });
    else if (t === "leave") cb({ type: "leave" });
    else if (t === "drop") cb({ type: "drop", paths: e.payload.paths });
  });

// ---- other libraries: rekordbox XML, iTunes / Music Library.xml, Traktor NML, Serato crates, M3U / M3U8 in; M3U8 / CSV out ----

export type InteropSource = "rekordbox" | "itunes" | "traktor" | "serato" | "m3u";

/** What an import may do with a track's cue point (the device keeps one per track):
 *  "off" writes none, "empty" sets it only where the track has none, "overwrite" also replaces. */
export type CueMode = "off" | "empty" | "overwrite";

/** Which parts of a library file to apply. `playlist_indexes` null = every playlist. */
export type ImportOptions = { ratings: boolean; playlists: boolean; playlist_indexes: number[] | null; cues: CueMode };

export type RatingChange = { track_id: number; title: string; artist: string; from: number; to: number };
export type PlannedTrack = { track_id: number; title: string; artist: string };
export type CaseAction = "create" | "extend" | "unchanged" | "skip";
export type CasePlan = {
  index: number;
  name: string;
  action: CaseAction;
  case_id: number | null;
  /** Tracks this step adds, in playlist order. */
  tracks: PlannedTrack[];
  already_in: number;
  unmatched: number;
  selected: boolean;
};
/** "set": the track has no cue; "replace": it has a different one; "same": it already has this one. */
export type CueAction = "set" | "replace" | "same";
export type CueNote = {
  track_id: number;
  title: string;
  artist: string;
  cue_secs: number;
  /** The cue in source frames at the track's sample rate (what the device row stores). */
  cue_frames: number;
  existing_frames: number | null;
  /** The track's sample rate: the unit of `cue_frames` and `existing_frames`. */
  sample_rate: number;
  action: CueAction;
};
export type UnmatchedTrack = {
  title: string;
  artist: string;
  path: string;
  reason: string;
  /** False when the location is a network, device, URL or relative path: it is never looked up. */
  path_checked: boolean;
  on_disk: boolean;
  in_playlist: boolean;
};

/** Everything an import would change, computed without writing. */
export type ImportPlan = {
  source: InteropSource;
  foreign_tracks: number;
  matched: number;
  matched_by_file: number;
  matched_by_tags: number;
  ratings: RatingChange[];
  ratings_unchanged: number;
  cases: CasePlan[];
  cues: CueNote[];
  /** What the reader skipped (damaged crates, smart crates...). */
  notes: string[];
  unmatched_total: number;
  /** The first 1000 unmatched tracks, those in playlists first. */
  unmatched: UnmatchedTrack[];
  /** Unmatched files that exist on this computer (what add-tracks could bring over). */
  addable_paths: string[];
};

export type ExportFormat = "m3u8" | "csv" | "rekordbox";
export type ExportScope = { scope: "library" } | { scope: "case" | "mix"; id: number };

/** Read a library file and list what importing it would change. Writes nothing. */
export const interopPreview = (root: string, path: string, options: ImportOptions) =>
  invoke<ImportPlan>("interop_preview", { root, path, options });

/** Apply the previewed import as a background job (`job-progress` / `job-done` with kind
 *  "interop"). Returns when the job has started. */
export const interopApply = (root: string, path: string, options: ImportOptions) =>
  invoke<void>("interop_apply", { root, path, options });

/** Write the whole library (rekordbox) or a case / mix / the library as a list (M3U8, CSV). */
export const interopExport = (root: string, format: ExportFormat, where: ExportScope, dest: string) =>
  invoke<{ path: string; tracks: number; notes?: string[] }>("interop_export", {
    root,
    format,
    scope: where.scope,
    id: "id" in where ? where.id : null,
    dest,
  });

/** What the file picker offers for each kind of import. */
export type PickKind = "rekordbox" | "itunes" | "traktor" | "m3u" | "serato-crate" | "serato-folder";

const PICKERS: Record<PickKind, { title: string; filter?: { name: string; extensions: string[] } }> = {
  rekordbox: { title: "Choose a rekordbox XML export", filter: { name: "rekordbox XML", extensions: ["xml"] } },
  itunes: { title: "Choose an iTunes / Music Library.xml", filter: { name: "Library export (XML)", extensions: ["xml"] } },
  traktor: { title: "Choose a Traktor collection.nml", filter: { name: "Traktor collection", extensions: ["nml"] } },
  m3u: { title: "Choose an M3U / M3U8 playlist", filter: { name: "Playlist", extensions: ["m3u8", "m3u"] } },
  "serato-crate": { title: "Choose a Serato .crate file", filter: { name: "Serato crate", extensions: ["crate"] } },
  "serato-folder": { title: "Choose the _Serato_ folder (it holds the Subcrates)" },
};

/** Pick the file (or, for Serato's folder, the folder) to import from (null if cancelled). */
export async function pickLibraryFile(kind: PickKind): Promise<string | null> {
  const p = PICKERS[kind];
  const sel = await open({
    multiple: false,
    directory: kind === "serato-folder",
    title: p.title,
    filters: p.filter ? [p.filter] : undefined,
  });
  return typeof sel === "string" ? sel : null;
}

/** Where to save an export (null if cancelled). In the browser preview, which has no native
 *  dialog for saving, a fixed placeholder path is returned. */
export async function pickSavePath(defaultName: string, label: string, ext: string): Promise<string | null> {
  try {
    const p = await save({ title: `Save ${label}`, defaultPath: defaultName, filters: [{ name: label, extensions: [ext] }] });
    return typeof p === "string" ? p : null;
  } catch (e) {
    if (String(e).includes("devmock")) return `C:\\Exports\\${defaultName}`;
    throw e;
  }
}

// ---- watch folders (src-tauri/src/commands/import/watch.rs) ----
// Folders on this computer that are searched for audio the library does not have. The settings,
// the last scan, the ignore list and the log live in the app's config folder, never on the device.

export type WatchMode = "ask" | "auto";

export type WatchSettings = {
  folders: string[];
  mode: WatchMode;
  scan_on_open: boolean;
  /** Minutes between scans while the app runs; 0 = never. */
  interval_minutes: number;
  subfolders: boolean;
  /** A file changed less than this many seconds ago waits for the next scan. */
  min_age_secs: number;
  /** "auto" adds at most this many files at once. */
  auto_max_files: number;
  /** In "ask" mode: open the confirmation as soon as a scan finds files. */
  open_confirmation: boolean;
  /** Same title and artist count as the same track when the lengths differ by at most this many seconds. */
  length_tolerance_secs: number;
};

export type WatchSettingsView = { settings: WatchSettings; problem: string | null };

export type WatchCandidate = { path: string; name: string; size: number; modified_ms: number; folder: string };
export type WatchMatched = { path: string; name: string; track_id: number; how: string };
export type WatchFolderReport = { path: string; error: string | null; audio_files: number };

export type WatchScan = {
  at: number;
  root: string;
  folders: WatchFolderReport[];
  /** New files, newest first (at most 5000; see `candidates_total`). */
  candidates: WatchCandidate[];
  candidates_total: number;
  audio_files: number;
  in_library: number;
  matched_total: number;
  matched: WatchMatched[];
  ignored: number;
  too_new: number;
  duplicates: number;
  unusable_names: number;
  cut_short: boolean;
};

export type WatchIgnored = { path: string; size: number; at: number };
export type WatchLogEntry = {
  at: number;
  mode: "auto" | "ask";
  requested: number;
  names: string[];
  ok: number;
  failed: number;
  summary: string;
};

export type WatchState = {
  ignored: WatchIgnored[];
  last_scan: WatchScan | null;
  log: WatchLogEntry[];
  /** Set when the saved file was damaged and had to be set aside. */
  problem: string | null;
};

export const watchSettingsGet = () => invoke<WatchSettingsView>("watch_settings_get");
/** Saves (clamped) and returns the settings as stored. */
export const watchSettingsSet = (settings: WatchSettings) => invoke<WatchSettings>("watch_settings_set", { settings });
/** Searches the saved folders for audio the library on `root` lacks. Reads only. */
export const watchScan = (root: string) => invoke<WatchScan>("watch_scan", { root });
export const watchState = (root: string) => invoke<WatchState>("watch_state", { root });
export const watchIgnore = (root: string, files: { path: string; size: number }[]) =>
  invoke<WatchState>("watch_ignore", { root, files });
export const watchUnignore = (root: string, paths: string[]) => invoke<WatchState>("watch_unignore", { root, paths });
export const watchLogAdd = (
  root: string,
  entry: { mode: "auto" | "ask"; requested: number; names: string[]; ok: number; failed: number; summary: string },
) => invoke<WatchState>("watch_log_add", { root, entry });

/** The folder the user picks in the native dialog (null if cancelled). */
export async function pickWatchFolder(): Promise<string | null> {
  const sel = await open({ directory: true, multiple: false, title: "Choose a folder to watch for new music" });
  return typeof sel === "string" ? sel : null;
}
