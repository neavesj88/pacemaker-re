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

/** How a group combines its items: all (AND), any (OR) or none (NOT any). */
export type Mode = "all" | "any" | "none";
export type TextField = "title" | "artist" | "album" | "genre" | "label" | "comments" | "remixer";
export type TextOp = "contains" | "is" | "starts_with";

/** One condition, or a group of them (groups nest). The old `genre` rule is rewritten by the
 * backend into `text` on load, so it never arrives here. */
export type Rule =
  | { kind: "rating_min"; stars: number }
  | { kind: "favourite"; value: boolean }
  | { kind: "plays_zero" }
  | { kind: "plays_min"; n: number }
  | { kind: "plays_range"; min: number | null; max: number | null }
  | { kind: "played_within"; days: number }
  /** Last played more than `days` days ago (stale); never-played tracks count when `include_never`. */
  | { kind: "last_played_before"; days: number; include_never: boolean }
  | { kind: "bpm_range"; min: number | null; max: number | null }
  | { kind: "key"; key: string }
  | { kind: "key_compatible"; key: string }
  | { kind: "analysed"; value: boolean }
  | { kind: "text"; field: TextField; op: TextOp; text: string }
  | { kind: "year_range"; min: number | null; max: number | null }
  | { kind: "added_within"; days: number }
  | { kind: "added_before"; days: number }
  | { kind: "duration_range"; min_secs: number | null; max_secs: number | null }
  | { kind: "format"; value: string }
  | { kind: "in_case"; case_id: number; value: boolean; case_name?: string }
  | { kind: "group"; mode: Mode; items: Rule[] };

export type SortField =
  | "title" | "artist" | "album" | "genre" | "label" | "bpm" | "key" | "rating" | "year"
  | "duration" | "date_added" | "plays" | "last_played" | "format" | "file_size";

export type Sort = { field: SortField; descending: boolean };

/** What a smart case asks for: `mode` over `rules`, then the order, then "only the first N".
 * No order = artist, title. No limit = every match. */
export type Query = { mode: Mode; rules: Rule[]; sort: Sort | null; limit: number | null };

export type SmartCase = Query & {
  /** 0 for a new one; the backend assigns the id on save. */
  id: number;
  name: string;
  /** "Put on Pacemaker" is on: a real case with the matches exists on the device. */
  on_device: boolean;
  device_case_id: number | null;
};

/** `error` is set when a smart case was skipped (a refresh of all of them) and nothing was written. */
export type DeviceCopy = { smart_case: SmartCase; count: number; error: string | null };

/** The live result of a query. */
export type Preview = {
  /** Track ids in display order, after the limit. */
  ids: number[];
  /** How many tracks matched before the limit cut the list. */
  matched: number;
  /** For an order by a column the track table lacks (year, date added ...): each track's value, aligned with `ids`; else empty. */
  values: string[];
};

export const listSmartCases = (root: string) => invoke<SmartCase[]>("smart_cases_list", { root });

/** Match count of each saved smart case right now. */
export const smartCaseCounts = (root: string) =>
  invoke<{ id: number; count: number; error: string | null }[]>("smart_cases_counts", { root });

/** The tracks a query matches, in its order, and how many matched before its limit. Saves nothing. */
export const evaluateRules = (root: string, query: Query) => invoke<Preview>("smart_cases_evaluate", { root, query });

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

// ---- keeping the Pacemaker copies current, settings, export -----------------------------------

/** A track named in a refresh summary. */
export type TrackRef = { track_id: number; title: string; artist: string };

/** What a refresh does (or did) to one smart case's Pacemaker copy. `create`: it does not exist
 * (never made, or deleted on the Pacemaker) and is made; `update`: tracks, their order or its name
 * differ; `unchanged`; `skip`: it cannot be refreshed (see `error`), nothing is written for it. */
export type RefreshAction = "create" | "update" | "unchanged" | "skip";

export type RefreshItem = {
  /** The smart case. */
  id: number;
  name: string;
  /** What its Pacemaker copy is called. */
  device_name: string;
  action: RefreshAction;
  /** Tracks in the copy before, and after the refresh. */
  before: number;
  after: number;
  added: TrackRef[];
  removed: TrackRef[];
  /** The same tracks in another order. */
  reordered: boolean;
  /** The copy's current name when it is not `device_name`. */
  renamed_from: string | null;
  /** The smart case had a copy once, but it was deleted (or replaced) on the Pacemaker. */
  recreated: boolean;
  error: string | null;
};

export type RefreshReport = {
  items: RefreshItem[];
  /** Items that are (or were) written: `create` and `update`. */
  changes: number;
};

/** What a refresh would change, by smart case, with the tracks named. Reads only. */
export const smartRefreshPlan = (root: string) => invoke<RefreshReport>("smart_cases_refresh_plan", { root });

/** Refreshes the Pacemaker copies that are out of date and returns what was done. Writes nothing
 * when every copy is current. */
export const smartRefreshApply = (root: string) => invoke<RefreshReport>("smart_cases_refresh_apply", { root });

/** Off: only the button. Ask: say what would change and wait. Auto: write it, then say what changed. */
export type RefreshMode = "off" | "ask" | "auto";

export type SmartSettings = {
  refresh_mode: RefreshMode;
  /** Library rekordbox XML: smart cases as playlists of their current matches. */
  export_smart: boolean;
  /** ...inside a "Smart cases" folder rather than beside the cases. */
  export_folder: boolean;
};

export type SmartSettingsState = { settings: SmartSettings; warning: string | null };

export const getSmartSettings = () => invoke<SmartSettingsState>("smart_settings_get");
export const setSmartSettings = (settings: SmartSettings) => invoke<SmartSettingsState>("smart_settings_set", { settings });

/** Writes one smart case, as it matches right now, as M3U8, CSV or rekordbox XML. */
export const exportSmartCase = (root: string, id: number, format: CaseExportFormat, dest: string) =>
  invoke<{ path: string; tracks: number; notes?: string[] }>("smart_cases_export", { root, id, format, dest });

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
export type TransferNewDone = TransferDone & { case_id: number };

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

/** Like `transferTracks` into a case created by the same write: if the transfer fails no empty
 * case is left behind. */
export const transferToNewCase = (root: string, from: number | null, newName: string, trackIds: number[], removeFromSource: boolean) =>
  invoke<TransferNewDone>("cases_transfer_new", { root, from, newName, trackIds, removeFromSource });

/** Deletes several cases in one transaction (the tracks stay); returns how many went. */
export const deleteCases = (root: string, caseIds: number[]) => invoke<number>("cases_delete_many", { root, caseIds });

export type CaseExportFormat = "m3u8" | "csv" | "rekordbox";

/** Writes one case to `dest` (a file outside the device volume) as M3U8, CSV or rekordbox XML. */
export const exportCase = (root: string, caseId: number, format: CaseExportFormat, dest: string) =>
  invoke<{ path: string; tracks: number; notes?: string[] }>("cases_export", { root, caseId, format, dest });
