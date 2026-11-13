// Duplicate finder commands (src-tauri/src/commands/cases/dupes.rs). Argument keys are camelCase;
// fields inside an object argument keep their Rust (snake_case) names.
import { invoke } from "@tauri-apps/api/core";

/** The two jobs' `job` names in job-progress / job-done events. */
export const DUPES_FINGERPRINT_JOB = "dupes-fingerprint";
export const DUPES_MERGE_JOB = "dupes-merge";

export type KeeperRule = "bitrate" | "plays" | "newest";
export type RatingRule = "max" | "keeper";
export type PlaysRule = "max" | "sum" | "keeper";

/** The finder's settings (the app's own preferences, not on the device). */
export type DupSettings = {
  by_tags: boolean;
  by_size: boolean;
  by_audio: boolean;
  strip_feat: boolean;
  ignore_brackets: boolean;
  ignore_the: boolean;
  /** 0 turns the length check of the tag way off. */
  tag_duration_secs: number;
  size_duration_secs: number;
  /** 0.5 to 1. */
  audio_threshold: number;
  audio_duration_secs: number;
  audio_offset_secs: number;
  /** 0.5 to 1: the share of the shorter track that must line up with the other for a sound match. */
  audio_min_overlap: number;
  /** 0 = pick by the machine. */
  fingerprint_threads: number;
  keeper_rule: KeeperRule;
  move_cases: boolean;
  rating_rule: RatingRule;
  plays_rule: PlaysRule;
  /** Whether a merge may take the other copies' rows out of the library (untried on hardware, so off at first). */
  allow_row_removal: boolean;
};

export type SettingsState = { settings: DupSettings; warning: string | null };

export type FileState = "present" | "missing" | "outside";
export type Mode = "tags" | "size" | "audio";

export type CaseHold = { case_id: number; name: string; position: number; of: number };

/** One track of a group: its whole row, the cases and mixes that use it, and the state of its file. */
export type Member = {
  track_id: number;
  title: string;
  artist: string;
  album_artist: string;
  album: string;
  composer: string;
  genre: string;
  label: string;
  key: string;
  comments: string;
  producer: string;
  remixer: string;
  year: number;
  track_number: number;
  disc_number: number;
  format: string;
  bit_rate_kbps: number;
  sample_rate: number;
  file_size: number;
  play_time_secs: number;
  bpm: number;
  rating: number;
  times_played: number;
  /** Unix milliseconds, -1 = never. */
  last_played: number;
  date_added: number;
  location: string;
  analysed: boolean;
  cases: CaseHold[];
  mixes: string[];
  file: FileState;
};

export type Link = { a: number; b: number; mode: Mode; score: number | null; offset_secs: number | null };

export type Group = {
  id: number;
  members: Member[];
  links: Link[];
  modes: Mode[];
  suggested_keeper: number;
};

export type FindResult = {
  groups: Group[];
  /** Tracks whose fingerprint is not cached yet (sound check on). */
  pending: number[];
  fingerprinted: number;
  tracks: number;
  warnings: string[];
};

export type MergeGroup = { keeper: number; others: number[] };

export type MergeRequest = {
  groups: MergeGroup[];
  remove_rows: boolean;
  delete_files: boolean;
  settings: DupSettings;
};

export type CaseEditKind = "replace" | "already_there" | "drop";
export type CaseEdit = { case_id: number; case_name: string; kind: CaseEditKind; others: number[] };
export type Change = { before: number; after: number };
export type StatEdit = { rating: Change; times_played: Change; last_played: Change };

export type GroupPlan = {
  keeper: number;
  others: number[];
  case_edits: CaseEdit[];
  stats: StatEdit;
  remove: { track_id: number; location: string }[];
  blocked: { track_id: number; mixes: string[] }[];
};

export type FileItem = {
  track_id: number;
  path: string;
  kind: "audio" | "grid";
  size: number;
  exists: boolean;
  /** Set when the file will be left alone, and why. */
  kept_because: string | null;
};

export type MergePlan = {
  groups: GroupPlan[];
  labels: { track_id: number; label: string }[];
  files: FileItem[];
  notes: string[];
  /** Things that stop the merge (shown in red, the Merge button stays off), e.g. the kept track has no file while a deleted copy does. */
  problems: string[];
  totals: { groups: number; case_entries: number; rows_removed: number; files_deleted: number; bytes: number };
  digest: string;
};

export const getDupSettings = () => invoke<SettingsState>("dupes_settings_get");

export const setDupSettings = (settings: DupSettings) => invoke<SettingsState>("dupes_settings_set", { settings });

/** Finds groups of copies with the ways the settings turn on. Reads only. */
export const findDuplicates = (root: string, settings: DupSettings) => invoke<FindResult>("dupes_find", { root, settings });

/** Starts the fingerprint job (events of job `dupes-fingerprint`); resolves with how many tracks it will do. */
export const startFingerprints = (root: string, settings: DupSettings) => invoke<number>("dupes_fingerprint_start", { root, settings });

export const cancelFingerprints = () => invoke<void>("dupes_cancel");

/** What the merge would do, file by file. Reads only. */
export const planMerge = (root: string, request: MergeRequest) => invoke<MergePlan>("dupes_merge_plan", { root, request });

/** Starts the merge job (events of job `dupes-merge`). Nothing is written if the library no
 *  longer matches the plan whose `digest` is passed. */
export const startMerge = (root: string, request: MergeRequest, digest: string) =>
  invoke<void>("dupes_merge_start", { root, request, digest });
