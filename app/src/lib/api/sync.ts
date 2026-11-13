// Library sync commands (src-tauri/src/commands/device/sync.rs): merge the tracks, mixes and cases
// of one library into another, in either direction ("Send to Pacemaker" / "Get from Pacemaker").
// The engine is pm_formats::ops_sync. The preview is read-only; the run is a background job of
// kind "device-sync" (progress on job-progress, the result sentence on job-done, stopped with
// deviceCancel from api/device.ts). After job-done the open library should be reloaded when it is
// the target.
import { invoke } from "@tauri-apps/api/core";

export const JOB_SYNC = "device-sync";

/** Newer: take the source's metadata when its row is newer. Source: always. Leave: never. */
export type SyncTracksOnBoth = "newer" | "leave" | "source";
export type SyncMixesSameName = "newer" | "replace" | "keep_both" | "skip";
/** Merge: keep the target's order and append the source's missing tracks. */
export type SyncCasesSameName = "merge" | "replace" | "skip";
export type SyncPlayCounts = "higher" | "leave";

/** Kept in the app's config folder, never on a device. Every value is a choice with a default. */
export type SyncSettings = {
  tracks_on_both: SyncTracksOnBoth;
  mixes_same_name: SyncMixesSameName;
  cases_same_name: SyncCasesSameName;
  /** Plan to remove what the target has and the source has not. Nothing is removed while this is off. */
  remove_missing: boolean;
  play_counts: SyncPlayCounts;
  /** Tags, colour labels and energy of a copied track follow it to the other library. */
  labels_follow: boolean;
};

export const DEFAULT_SYNC_SETTINGS: SyncSettings = {
  tracks_on_both: "newer",
  mixes_same_name: "newer",
  cases_same_name: "merge",
  remove_missing: false,
  play_counts: "higher",
  labels_follow: true,
};

export type SyncSettingsView = { settings: SyncSettings; warning: string | null };

export type SyncItemKind = "track" | "mix" | "case";

export type SyncAction =
  | { type: "copy" }
  | { type: "update" }
  /** Only the play counts come over; no file is copied. */
  | { type: "update_plays" }
  | { type: "replace" }
  | { type: "copy_as_new"; name: string }
  | { type: "merge" }
  | { type: "skip"; reason: string }
  | { type: "remove" };

export type SyncNeed = { location: string; label: string };
export type SyncBlocker = { key: string; label: string };

export type SyncItem = {
  /** Stable: `track:<location>`, `mix:<name>`, `case:<name>` (names trimmed, lower case), and `remove-track:`, `remove-mix:`, `remove-case:`. */
  key: string;
  kind: SyncItemKind;
  action: SyncAction;
  /** "Artist - Title", or the mix or case name. */
  label: string;
  /** Bytes copied (audio plus the .str; an update copies the .str). */
  bytes: number;
  /** For a mix or case: the tracks it needs on the target. */
  needs: SyncNeed[];
  /** Extra words, e.g. how many case tracks are left out. */
  detail: string;
  global_id: string | null;
  location: string | null;
  source_id: number | null;
  target_id: number | null;
  /** For a track removal: the mixes and cases on the target that use it. */
  blockers: SyncBlocker[];
  /** For a track a selection plan pulled in (and that was not itself selected): the mixes and cases
   *  that need it, as `mix "Name"` / `case "Name"`. */
  needed_by: string[];
  /** For the item that puts the selected tracks into a case on the target: that case's name. Its key
   *  is `into-case:<name>`. */
  into_case: string | null;
};

export type SyncPlan = {
  items: SyncItem[];
  settings: Omit<SyncSettings, "labels_follow">;
  /** "computer" or "Pacemaker". */
  source_label: string;
  bytes_to_copy: number;
  /** Free bytes on the target volume, null when they could not be read. */
  free_bytes: number | null;
  fits: boolean;
  /** Goes back with syncApply; the run refuses if either library changed since. */
  digest: string;
  /** For example that smart cases are not synced. */
  notes: string[];
};

/** What was picked on the sending side: track locations, mix names, case names, and optionally the
 *  target case the picked tracks should end up in (made when `create` and the target has none). */
export type SyncSelection = {
  tracks: string[];
  mixes: string[];
  cases: string[];
  into_case: { name: string; create: boolean } | null;
};

export const syncSettingsGet = () => invoke<SyncSettingsView>("sync_settings_get");
export const syncSettingsSet = (settings: SyncSettings) => invoke<SyncSettingsView>("sync_settings_set", { settings });
/** Read-only. Uses the saved settings. */
export const syncPlan = (source: string, target: string) => invoke<SyncPlan>("sync_plan", { source, target });
/** Read-only. What copying only the picked items would do, using the saved settings. Looks up the picked
 *  items only, so it is quick on a library of any size. Never removes anything. */
export const syncPlanSelection = (source: string, target: string, selection: SyncSelection) =>
  invoke<SyncPlan>("sync_plan_selection", { source, target, selection });
/** Job "device-sync": runs exactly the plan syncPlanSelection returned (same selection, its digest).
 *  Refused as stale only when a selected item moved, not when something else in a library did. */
export const syncApplySelection = (source: string, target: string, selection: SyncSelection, digest: string) =>
  invoke<void>("sync_apply_selection", { source, target, selection, digest });
/** Job "device-sync": runs the plan the user saw minus the unticked `excluded` keys. A mix whose tracks were unticked is left out with a reason. */
export const syncApply = (source: string, target: string, digest: string, excluded: string[]) =>
  invoke<void>("sync_apply", { source, target, digest, excluded });
