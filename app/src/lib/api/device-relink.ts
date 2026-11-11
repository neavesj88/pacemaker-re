// Missing-file relocation commands of the device area (src-tauri/src/commands/device/relink.rs):
// find the audio of tracks whose file is gone in a folder (PC or card), preview each match, then
// point the track at the file or copy it onto the card first. Re-exported from api/device.ts. The
// run is a background job of kind "device-relink"; the search reports through its own event.
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";

export const JOB_RELINK = "device-relink";
export const RELINK_SCAN_PROGRESS = "relink-scan-progress";

/** Which matches start ticked. */
export type RelinkPreselect = "none" | "sure" | "likely";

/** Kept in the app's config folder, never on the card. */
export type RelinkSettings = {
  /** Also read title, artist and length from files that do not match by name. */
  use_tags: boolean;
  /** How far a file's length may differ from the track's, in seconds. */
  duration_tolerance_secs: number;
  /** Stop reading tags after this many files. */
  max_tag_reads: number;
  subfolders: boolean;
  /** Stop listing a folder after this many audio files. */
  max_files: number;
  /** A match already under Music: point at it where it is, or copy it to the app's naming. */
  card_files: "reuse" | "standard";
  preselect: RelinkPreselect;
  /** The folder searched last time. */
  last_folder: string;
};

export type RelinkSettingsView = { settings: RelinkSettings; warning: string | null };

/** exact: same file name and size as when the track was added. same_name: same name, another size.
 *  tags: same title, artist and length. manual: picked by hand with no evidence. */
export type RelinkKind = "exact" | "same_name" | "tags" | "manual";
export type RelinkConfidence = "sure" | "likely" | "check";

export type RelinkCandidate = {
  /** The file's full path. */
  path: string;
  name: string;
  size: number;
  kind: RelinkKind;
  confidence: RelinkConfidence;
  /** The evidence in words. */
  notes: string[];
  duration_secs: number | null;
  /** point: the file is already under Music, only the track's location changes. copy: it is copied onto the card first. */
  action: "point" | "copy";
  /** Where it will be on the card, relative to .Pacemaker/Music. */
  dest: string;
  /** The track's location after the relink; sent back with the run. */
  new_location: string;
  /** The file is already under .Pacemaker/Music. */
  on_card: boolean;
  /** The track would point at a path the app did not generate (not verified on hardware). */
  non_standard: boolean;
};

export type RelinkRow = {
  track_id: number;
  label: string;
  /** The location the track has now (the file is not there). */
  location: string;
  file_size: number;
  play_time_secs: number;
  /** tracks.format as recorded. */
  format: string;
  title: string;
  artist: string;
  in_mixes: string[];
  /** Best first; empty when nothing in the folder fits. */
  candidates: RelinkCandidate[];
  /** Files that fit but cannot be used, and why. */
  notes: string[];
};

export type RelinkPlan = {
  folder: string;
  total_missing: number;
  files_seen: number;
  tag_reads: number;
  truncated: boolean;
  tag_limit_hit: boolean;
  rows: RelinkRow[];
  warnings: string[];
  /** Tracks left out because their location is not a plain path under Music. */
  skipped_locations: number;
  /** The card has no Music folder: every track is missing, copying creates it. */
  music_missing: boolean;
  /** Files whose tags were read but gave no length. */
  tags_without_length: number;
  /** Some match points at a path the app did not generate. */
  non_standard: boolean;
};

/** One ticked match. `expect_location` is the candidate's `new_location` from the preview. */
export type RelinkPick = { track_id: number; path: string; manual: boolean; expect_location: string };

export type RelinkScanProgress = {
  /** 0: listing folders, 1: comparing files. */
  phase: number;
  done: number;
  total: number;
  label: string;
};

export const deviceRelinkSettings = () => invoke<RelinkSettingsView>("device_relink_settings");
export const deviceRelinkSettingsSet = (settings: RelinkSettings) => invoke<RelinkSettingsView>("device_relink_settings_set", { settings });
/** Read-only. A stopped search rejects with a message starting "Cancelled". */
export const deviceRelinkScan = (root: string, folder: string, settings: RelinkSettings) =>
  invoke<RelinkPlan>("device_relink_scan", { root, folder, settings });
export const deviceRelinkScanCancel = () => invoke<void>("device_relink_scan_cancel");
/** One file picked by hand for one track: the evidence and what taking it would do. */
export const deviceRelinkInspect = (root: string, trackId: number, path: string, settings: RelinkSettings) =>
  invoke<RelinkCandidate>("device_relink_inspect", { root, trackId, path, settings });
/** Job "device-relink". Each pick is checked again on the fresh library before anything changes. Stopped with deviceCancel. */
export const deviceRelinkRun = (root: string, picks: RelinkPick[], settings: RelinkSettings) =>
  invoke<void>("device_relink_run", { root, picks, settings });
export const onRelinkScanProgress = (cb: (e: RelinkScanProgress) => void) =>
  listen<RelinkScanProgress>(RELINK_SCAN_PROGRESS, (e) => cb(e.payload));

export async function pickRelinkFolder(): Promise<string | null> {
  const d = await open({ directory: true, title: "Choose the folder to search for the missing audio" });
  return typeof d === "string" ? d : null;
}

export async function pickRelinkFile(): Promise<string | null> {
  const f = await open({
    title: "Choose the audio file for this track",
    filters: [{ name: "Audio", extensions: ["mp3", "flac", "wav", "m4a", "aac", "ogg", "opus", "aif", "aiff", "wma", "mp4"] }],
  });
  return typeof f === "string" ? f : null;
}
