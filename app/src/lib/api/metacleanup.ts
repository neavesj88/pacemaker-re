// Metadata clean-up and tag write-back commands (src-tauri/src/commands/metadata/{cleanup,tagwrite}.rs).
import { invoke } from "@tauri-apps/api/core";
import type { TrackEdit } from "./metadata";

export type MapEntry = { from: string; to: string };

/** Every choice of the clean-up tool; saved in the app config dir, never on the device. */
export type CleanupSettings = {
  trim: boolean;
  title_case: boolean;
  case_fields: string[];
  keep_upper: string[];
  /** Words Title Case keeps in lower case inside a title (a, of, the, feat., ...). */
  small_words: string[];
  strip_number: boolean;
  /** The most digits a leading number may have to count as a track number. */
  number_max_digits: number;
  number_space_only: boolean;
  strip_promo: boolean;
  promo_fields: string[];
  promo_phrases: string[];
  feat_normalise: boolean;
  /** "feat." | "feat" | "ft." | "featuring" */
  feat_form: string;
  feat_to_artist: boolean;
  extract_remixer: boolean;
  remix_words: string[];
  remixer_ignore: string[];
  overwrite_remixer: boolean;
  genre_map_on: boolean;
  genre_map: MapEntry[];
  artist_map_on: boolean;
  artist_map: MapEntry[];
  /** What separates names in an artist field: "," goes straight after a name, the rest have a space either side. */
  artist_separators: string[];
  /** Also write the changed fields into the audio files' own tags. Off unless asked for. */
  write_back: boolean;
  /** The fields the stand-alone "write library tags to files" tool starts with. */
  write_fields: string[];
};

export const FEAT_FORMS = ["feat.", "feat", "ft.", "featuring"];

export type CleanupChange = {
  field: string;
  old: string;
  new: string;
  /** Why this change cannot be applied (it would not fit the device's column). */
  problem: string | null;
};

export type CleanupRow = { track_id: number; artist: string; title: string; changes: CleanupChange[] };
export type CleanupPreview = { scanned: number; rows: CleanupRow[] };
export type AcceptedChange = { track_id: number; field: string; old: string; new: string };
export type ApplyResult = { tracks: number; fields: number; stale: number; rejected: number };

export const cleanupSettings = (): Promise<{ settings: CleanupSettings; warning: string | null }> =>
  invoke("metadata_cleanup_settings");

/** The built-in recipes and maps (nothing is saved by asking). */
export const cleanupDefaults = () => invoke<CleanupSettings>("metadata_cleanup_defaults");

export const saveCleanupSettings = (settings: CleanupSettings) =>
  invoke<CleanupSettings>("metadata_cleanup_save_settings", { settings });

export const previewCleanup = (root: string, trackIds: number[], settings: CleanupSettings) =>
  invoke<CleanupPreview>("metadata_cleanup_preview", { root, trackIds, settings });

/** Writes the accepted changes to music.db; the backend re-checks each against the library first. */
export const applyCleanup = (root: string, settings: CleanupSettings, accepted: AcceptedChange[]) =>
  invoke<ApplyResult>("metadata_cleanup_apply", { root, settings, accepted });

// ---- write-back into the audio files' own tags ----

/** One track's values to write into its file; only the fields present are looked at. */
export type TagItem = { track_id: number } & TrackEdit;

export type TagChange = { field: string; old: string; new: string };

export type TagPlan = {
  track_id: number;
  label: string;
  /** The file below the Music folder, e.g. "3/a/1f2e.mp3". */
  file: string;
  /** "MP3 (ID3v2)"; "" when the file could not be opened. */
  format: string;
  /** "write" | "same" | "unsupported" | "missing" | "error" */
  status: string;
  changes: TagChange[];
  unsupported_fields: string[];
  /** Things to know before agreeing, e.g. that an ID3v2.2 tag is upgraded to 2.4. */
  notes: string[];
  message: string;
};

export type TagFormat = { ext: string; container: string; tag: string };

/** The job kind of the tag write (progress and done events carry it). */
export const TAG_JOB_KIND = "tag-write";

/** What writing these values would do to each file (reads the files, changes nothing). */
export const planTags = (root: string, items: TagItem[]) => invoke<TagPlan[]>("metadata_tags_plan", { root, items });

export const tagFormats = () => invoke<TagFormat[]>("metadata_tags_formats");

/** Starts the write as a background job (kind "tag-write"); resolves once it has started. */
export const writeTags = (root: string, items: TagItem[]) => invoke<void>("metadata_tags_write", { root, items });
