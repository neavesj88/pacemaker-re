// Track commands (src-tauri/src/commands/tracks.rs).
import { invoke } from "@tauri-apps/api/core";

/** Rewrites the .str / BPM of these tracks from the audio on the volume. Returns when the job
 *  has started; progress arrives as job-progress / job-done events. */
export const reanalyseTracks = (root: string, trackIds: number[]) => invoke<void>("reanalyse_tracks", { root, trackIds });

/** Names of the mixes that have any of these tracks in them. Re-analysing replaces a track's grid
 *  and tempo, and a mix clip keeps the tempo the track had when the mix was made. Read-only. */
export const tracksMixesUsing = (root: string, trackIds: number[]) => invoke<string[]>("tracks_mixes_using", { root, trackIds });

/** Shows the track's audio file in the system file manager (Explorer with the file selected on
 *  Windows). Read-only. */
export const revealTrack = (root: string, trackId: number) => invoke<void>("tracks_reveal", { root, trackId });
