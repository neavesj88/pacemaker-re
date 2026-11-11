// Free-space planner commands of the device area (src-tauri/src/commands/device/space.rs): which
// tracks to archive or remove to reach a target, the archive to a PC folder and the removal from
// the card. Re-exported from api/device.ts. The run is a background job of kind "device-space".
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";

export const JOB_SPACE = "device-space";

/** Which tracks the planner nominates and in what order. Every value is a setting. */
export type SpaceRules = {
  /** Reason: never played. */
  never_played: boolean;
  /** Reason: not played for this many months (0 = off). Needs at least one play. */
  stale_months: number;
  /** Reason: rated 1 to this many stars (0 = off). */
  low_rating_max: number;
  unrated_is_low: boolean;
  /** Reason: another track holds the same audio, byte for byte. */
  duplicates: boolean;
  /** How many of the reasons a track needs. */
  min_reasons: number;
  only_no_case: boolean;
  only_no_mix: boolean;
  protect_favourites: boolean;
  /** Never suggest a track rated at least this many stars (0 = off). */
  protect_rating_min: number;
  /** Never suggest a track added in the last N days (0 = off). */
  protect_new_days: number;
  order: "certain" | "largest";
};

/** Kept in the app's config folder, never on the card. */
export type SpaceSettings = {
  rules: SpaceRules;
  goal_kind: "free" | "card";
  /** Free-space target in GB as the Storage card shows them (2^30 bytes). */
  free_gb: number;
  /** The card to fit, in GB as it is sold (10^9 bytes). */
  card_gb: number;
  reserve_percent: number;
  archive_dir: string;
  include_grids: boolean;
  /** Removing from the card needs an archive copy of the same tracks first. */
  require_archive: boolean;
};

export type SpaceSettingsView = { settings: SpaceSettings; warning: string | null };

export type ReasonKind = "never_played" | "not_played_recently" | "low_rating" | "duplicate" | "no_case" | "no_mix";
export type Reason = { kind: ReasonKind; text: string };

export type SpaceItem = {
  track_id: number;
  label: string;
  format: string;
  /** What removing it frees (audio plus beat grid). */
  bytes: number;
  play_time_secs: number;
  rating: number;
  times_played: number;
  last_played_ms: number | null;
  added_ms: number | null;
  in_cases: string[];
  in_mixes: string[];
  reasons: Reason[];
  duplicate_of: number | null;
  duplicate_of_label: string | null;
  /** Why it cannot be removed (it can still be archived). */
  blocked: string | null;
  /** In the prefix of the list that reaches the target. */
  suggested: boolean;
};

export type SpaceHidden = { favourites: number; high_rated: number; new_tracks: number };

export type SpacePlan = {
  rules_on: boolean;
  total_tracks: number;
  need_bytes: number;
  free_bytes: number;
  total_bytes: number;
  library_bytes: number;
  /** Card goal: the part of the card the library may fill. */
  usable_bytes: number | null;
  items: SpaceItem[];
  candidate_bytes: number;
  removable_bytes: number;
  suggested_count: number;
  suggested_bytes: number;
  reachable: boolean;
  hidden: SpaceHidden;
  missing_audio: number;
  unchecked: number;
  /** No track has a play count or date: the device may not record plays, so the play reasons say nothing. */
  no_play_history: boolean;
  warnings: string[];
};

export type SpaceRunItem = {
  track_id: number;
  label: string;
  bytes: number;
  /** The file name in the archive folder, when an archive is made. */
  archive_name: string | null;
  /** The beat grid's file name in the archive folder, when grids are archived and the track has one. */
  grid_name: string | null;
  /** The track and its files are removed from the card. */
  remove: boolean;
  /** Why it is not removed, in words. */
  note: string | null;
};

export type SpaceRunPlan = {
  items: SpaceRunItem[];
  skipped: string[];
  archive_bytes: number;
  remove_bytes: number;
  archive_free_bytes: number | null;
  /** Reasons the run cannot start. */
  problems: string[];
};

/** What a run is asked to do. `archive_dir` null: no archive. `require_archive` is the user's setting. */
export type SpaceRunArgs = { archive_dir: string | null; include_grids: boolean; remove: boolean; require_archive: boolean };

export const deviceSpaceSettings = () => invoke<SpaceSettingsView>("device_space_settings");
export const deviceSpaceSettingsSet = (settings: SpaceSettings) => invoke<SpaceSettingsView>("device_space_settings_set", { settings });
/** Read-only. Looking for duplicates reads files that share a size with another file. */
export const deviceSpacePlan = (root: string, settings: SpaceSettings) => invoke<SpacePlan>("device_space_plan", { root, settings });
/** What a run would do with each ticked track; the run repeats the same check before it acts. */
export const deviceSpaceRunPreview = (root: string, trackIds: number[], archiveDir: string | null, includeGrids: boolean, remove: boolean) =>
  invoke<SpaceRunPlan>("device_space_run_preview", { root, trackIds, archiveDir, includeGrids, remove });
/** Job "device-space": archive (verified copy), then remove what may go. Stopped with deviceCancel. */
export const deviceSpaceRun = (root: string, trackIds: number[], args: SpaceRunArgs) => invoke<void>("device_space_run", { root, trackIds, args });
export const deviceSpaceOpenFolder = (path: string) => invoke<void>("device_space_open_folder", { path });

export async function pickArchiveFolder(): Promise<string | null> {
  const d = await open({ directory: true, title: "Choose where to archive the tracks (not on the Pacemaker)" });
  return typeof d === "string" ? d : null;
}
