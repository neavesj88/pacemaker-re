// Case commands (src-tauri/src/commands/cases.rs). Argument keys are camelCase; fields inside an
// object argument keep their Rust (snake_case) names.
import { invoke } from "@tauri-apps/api/core";

/** A case with the metadata the Editor's case table and dialog show. */
export type CaseInfo = {
  id: number;
  name: string;
  genre: string;
  year: number;
  /** Stars, 0 (unrated) to 5. */
  rating: number;
  producer: string;
  remixer: string;
  comments: string;
  /** Unix milliseconds. */
  date_created: number;
  date_modified: number;
  times_played: number;
  /** Unix milliseconds, 0 = never. */
  last_played: number;
  play_time_secs: number;
  track_count: number;
};

/** What the case dialog edits. */
export type CaseMeta = Pick<CaseInfo, "name" | "genre" | "year" | "rating" | "producer" | "remixer" | "comments">;

/** Track ids in a case, in the order they were added. */
export const caseTracks = (root: string, caseId: number) => invoke<number[]>("case_tracks", { root, caseId });

/** Every case with its metadata, ordered by name. */
export const listCases = (root: string) => invoke<CaseInfo[]>("cases_list", { root });

/** Creates a case holding `trackIds` (may be empty); returns its id. */
export const createCase = (root: string, name: string, trackIds: number[] = []) =>
  invoke<number>("cases_create", { root, name, trackIds });

export const renameCase = (root: string, caseId: number, name: string) =>
  invoke<boolean>("cases_rename", { root, caseId, name });

export const setCaseMeta = (root: string, caseId: number, meta: CaseMeta) =>
  invoke<boolean>("cases_set_meta", { root, caseId, meta });

/** Deletes the case only; its tracks stay in the library. */
export const deleteCase = (root: string, caseId: number) => invoke<boolean>("cases_delete", { root, caseId });

/** Appends tracks to a case, skipping any already in it; returns how many were added. The tracks
 * view calls this for "Add to case". Callers should then `cases.refresh()` (state/cases.svelte.ts)
 * so counts and lengths update. */
export const addTracksToCase = (root: string, caseId: number, trackIds: number[]) =>
  invoke<number>("cases_add_tracks", { root, caseId, trackIds });

export const removeTracksFromCase = (root: string, caseId: number, trackIds: number[]) =>
  invoke<number>("cases_remove_tracks", { root, caseId, trackIds });

/** Replaces the case's tracks with exactly this list, in this order (reordering). */
export const setCaseTracks = (root: string, caseId: number, trackIds: number[]) =>
  invoke<void>("cases_set_tracks", { root, caseId, trackIds });

/** Add the track to the "Favourites" case (created on first use) or remove it; returns whether it
 * is now a favourite. Cross-area contract: the cases area implements `cases_toggle_favourite`. */
export const toggleFavourite = (root: string, trackId: number) =>
  invoke<boolean>("cases_toggle_favourite", { root, trackId });

/** Ids of the tracks in the "Favourites" case (empty if it doesn't exist yet). */
export const favouriteIds = (root: string) => invoke<number[]>("cases_favourite_ids", { root });

// ---- smart cases ------------------------------------------------------------------------------

/** One condition; a smart case matches the tracks for which every rule holds. */
export type Rule =
  | { kind: "rating_min"; stars: number }
  | { kind: "favourite"; value: boolean }
  | { kind: "plays_zero" }
  | { kind: "plays_min"; n: number }
  | { kind: "played_within"; days: number }
  | { kind: "bpm_range"; min: number | null; max: number | null }
  | { kind: "key"; key: string }
  | { kind: "key_compatible"; key: string }
  | { kind: "genre"; text: string }
  | { kind: "analysed"; value: boolean };

export type SmartCase = {
  /** 0 for a new one; the backend assigns the id on save. */
  id: number;
  name: string;
  rules: Rule[];
  /** "Put on Pacemaker" is on: a real case with the matches exists on the device. */
  on_device: boolean;
  device_case_id: number | null;
};

export type DeviceCopy = { smart_case: SmartCase; count: number };

export const listSmartCases = (root: string) => invoke<SmartCase[]>("smart_cases_list", { root });

/** Match count of each saved smart case right now. */
export const smartCaseCounts = (root: string) => invoke<{ id: number; count: number }[]>("smart_cases_counts", { root });

/** Track ids a rule set matches, in display order. Saves nothing. */
export const evaluateRules = (root: string, rules: Rule[]) => invoke<number[]>("smart_cases_evaluate", { root, rules });

/** Saves a new (id 0) or edited smart case in the app's config dir; never touches the device. */
export const saveSmartCase = (root: string, smartCase: SmartCase) =>
  invoke<SmartCase>("smart_cases_save", { root, smartCase });

/** Deletes a smart case; with `removeDeviceCase` also the case it made on the Pacemaker. */
export const deleteSmartCase = (root: string, id: number, removeDeviceCase: boolean) =>
  invoke<boolean>("smart_cases_delete", { root, id, removeDeviceCase });

/** The "Put on Pacemaker" toggle. On: create the device case or refresh it to the current
 * matches. Off: delete the device case. */
export const setSmartCaseOnDevice = (root: string, id: number, on: boolean) =>
  invoke<DeviceCopy>("smart_cases_set_on_device", { root, id, on });

/** Refreshes the device case of every smart case that has "Put on Pacemaker" on. */
export const refreshSmartDeviceCases = (root: string) => invoke<DeviceCopy[]>("smart_cases_refresh_all", { root });

// ---- organising: duplicate, merge, copy / move, delete several, export ------------------------

/** What a merge adds. Counts are tracks; `added` is the list that will be appended, in order. */
export type MergePlan = {
  added: number[];
  /** Source tracks the destination already holds. */
  already_there: number;
  /** Source tracks left out because an earlier source supplied them. */
  repeated: number;
  /** Source rows that point at a track no longer in the library (not copied). */
  stale: number;
};

export type MergeDone = { case_id: number; added: number; deleted_sources: number };

/** What copying or moving tracks between cases does. Every list holds track ids. */
export type TransferPlan = {
  to_add: number[];
  already_there: number[];
  /** Removed from the source (a move only). */
  to_remove: number[];
  not_in_source: number[];
  /** Ids that are not tracks of the library. */
  missing: number[];
};

export type TransferDone = { added: number; removed: number };

/** Where a merge goes: an existing case (kept as it is, new tracks appended) or a new one. */
export type MergeTarget = { existing: number } | { newName: string };

/** A copy of a case under `name` (tracks in order, metadata copied, play statistics reset);
 * returns the new case's id. */
export const duplicateCase = (root: string, caseId: number, name: string) =>
  invoke<number>("cases_duplicate", { root, caseId, name });

/** What `mergeCases` would add for the same arguments. Writes nothing. */
export const mergePreview = (root: string, sourceIds: number[], target: number | null) =>
  invoke<MergePlan>("cases_merge_preview", { root, sourceIds, target });

/** Merges cases without repeats, into an existing case or a new one; with `deleteSources` the
 * source cases are deleted in the same transaction (their tracks stay in the library). */
export const mergeCases = (root: string, sourceIds: number[], to: MergeTarget, deleteSources: boolean) =>
  invoke<MergeDone>("cases_merge", {
    root,
    sourceIds,
    target: "existing" in to ? to.existing : null,
    newName: "newName" in to ? to.newName : null,
    deleteSources,
  });

/** What `transferTracks` would do for the same arguments. `from` null = picked from the library. */
export const transferPreview = (root: string, from: number | null, to: number, trackIds: number[], removeFromSource: boolean) =>
  invoke<TransferPlan>("cases_transfer_preview", { root, from, to, trackIds, removeFromSource });

/** Copies (`removeFromSource` false) or moves tracks from case `from` into case `to`, appended
 * and skipping any it already holds. A move takes them out of `from` only. */
export const transferTracks = (root: string, from: number | null, to: number, trackIds: number[], removeFromSource: boolean) =>
  invoke<TransferDone>("cases_transfer", { root, from, to, trackIds, removeFromSource });

/** Deletes several cases in one transaction (the tracks stay); returns how many went. */
export const deleteCases = (root: string, caseIds: number[]) => invoke<number>("cases_delete_many", { root, caseIds });

export type CaseExportFormat = "m3u8" | "csv" | "rekordbox";

/** Writes one case to `dest` (a file outside the device volume) as M3U8, CSV or rekordbox XML. */
export const exportCase = (root: string, caseId: number, format: CaseExportFormat, dest: string) =>
  invoke<{ path: string; tracks: number }>("cases_export", { root, caseId, format, dest });
