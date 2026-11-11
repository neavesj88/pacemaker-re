// metadata commands (src-tauri/src/commands/metadata.rs).
import { invoke } from "@tauri-apps/api/core";

/** Set a track's star rating, 0 (unrated) to 5. Cross-area contract: the metadata area
 * implements `metadata_set_rating` (pm-formats Library::set_track_rating); the tracks table calls it. */
export const setRating = (root: string, trackId: number, stars: number) =>
  invoke<void>("metadata_set_rating", { root, trackId, stars });

/** The editable fields of one track, as the metadata dialog shows them. */
export type TrackDetail = {
  track_id: number;
  title: string;
  artist: string;
  album_artist: string;
  album: string;
  composer: string;
  genre: string;
  label: string;
  remixer: string;
  producer: string;
  comments: string;
  year: number;
  track_number: number;
  number_of_tracks: number;
  disc_number: number;
  number_of_discs: number;
  rating: number;
  /** Read-only here: set by analysis. */
  bpm: number;
  /** Read-only here: set by key detection. */
  key: string;
};

export const TEXT_FIELDS = [
  "title",
  "artist",
  "album_artist",
  "album",
  "composer",
  "genre",
  "label",
  "remixer",
  "producer",
  "comments",
] as const;
export const NUMBER_FIELDS = ["year", "track_number", "number_of_tracks", "disc_number", "number_of_discs"] as const;

export type TextField = (typeof TEXT_FIELDS)[number];
export type NumberField = (typeof NUMBER_FIELDS)[number];

/** Limits the device schema declares (VARCHAR(255), NUMBER(4)); the backend enforces the same. */
export const MAX_TEXT_CHARS = 255;
export const MAX_SMALL_NUMBER = 9999;

/** Only the fields that changed; an absent key leaves the field alone. */
export type TrackEdit = Partial<Record<TextField, string>> & Partial<Record<NumberField, number>> & { rating?: number };

export const trackDetails = (root: string, trackIds: number[]) =>
  invoke<TrackDetail[]>("metadata_track_details", { root, trackIds });

/** Writes all edits in one transaction (all or nothing); resolves to how many tracks changed. */
export const saveMetadata = (root: string, edits: (TrackEdit & { track_id: number })[]) =>
  invoke<number>("metadata_save", { root, edits });
