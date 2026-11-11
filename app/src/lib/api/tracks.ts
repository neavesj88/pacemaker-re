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

// ---- saved filter presets and the remembered view (app-side; see commands/tracks/views.rs) ----

/** A saved filter preset. `state` is what the Tracks view saved (views/tracks/viewState.ts reads it
 *  back safely); the backend only keeps it as a bounded JSON object. */
export type ViewPreset = { id: string; name: string; state: unknown };

export type ViewsPayload = {
  presets: ViewPreset[];
  /** The view the table was left in, or null. */
  state: unknown | null;
  /** Set when the file could not be read (presets then start empty). */
  notice: string | null;
};

export const tracksViewsLoad = (root: string) => invoke<ViewsPayload>("tracks_views_load", { root });

/** Saves `state` as `name`. A preset with that name (ignoring case) is replaced only when
 *  `replace` is set; otherwise the call fails. Returns the fresh list. */
export const tracksViewsSavePreset = (root: string, name: string, state: unknown, replace: boolean) =>
  invoke<ViewsPayload>("tracks_views_save_preset", { root, name, state, replace });

export const tracksViewsDeletePreset = (root: string, id: string) => invoke<ViewsPayload>("tracks_views_delete_preset", { root, id });

/** Remembers the view the table was left in; null forgets it. */
export const tracksViewsSetState = (root: string, state: unknown | null) => invoke<void>("tracks_views_set_state", { root, state });
