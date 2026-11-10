// Track commands (src-tauri/src/commands/tracks.rs).
import { invoke } from "@tauri-apps/api/core";

/** Rewrites the .str / BPM of these tracks from the audio on the volume. Returns when the job
 *  has started; progress arrives as job-progress / job-done events. */
export const reanalyseTracks = (root: string, trackIds: number[]) => invoke<void>("reanalyse_tracks", { root, trackIds });
