// metadata commands (src-tauri/src/commands/metadata.rs).
import { invoke } from "@tauri-apps/api/core";

/** Set a track's star rating, 0 (unrated) to 5. Cross-area contract: the metadata area
 * implements `metadata_set_rating` (pm-formats Library::set_track_rating); the tracks table calls it. */
export const setRating = (root: string, trackId: number, stars: number) =>
  invoke<void>("metadata_set_rating", { root, trackId, stars });
