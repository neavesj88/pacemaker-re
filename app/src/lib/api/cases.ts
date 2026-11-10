// Case commands (src-tauri/src/commands/cases.rs).
import { invoke } from "@tauri-apps/api/core";

/** Track ids in a case, in the order they were added. */
export const caseTracks = (root: string, caseId: number) => invoke<number[]>("case_tracks", { root, caseId });

/** Add the track to the "Favourites" case (created on first use) or remove it; returns whether it
 * is now a favourite. Cross-area contract: the cases area implements `cases_toggle_favourite`. */
export const toggleFavourite = (root: string, trackId: number) =>
  invoke<boolean>("cases_toggle_favourite", { root, trackId });

/** Ids of the tracks in the "Favourites" case (empty if it doesn't exist yet). */
export const favouriteIds = (root: string) => invoke<number[]>("cases_favourite_ids", { root });
