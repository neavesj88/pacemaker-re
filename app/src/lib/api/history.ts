// History commands (src-tauri/src/commands/history.rs).
import { invoke } from "@tauri-apps/api/core";

export type PlayEntry = { track_id: number; times_played: number; last_played: number };
/** Tracks whose last plays are close together, oldest first. start/end are Unix ms. */
export type PlaySession = { start: number; end: number; track_ids: number[]; played_at: number[] };
export type PlayedCollection = { id: number; name: string; times_played: number; last_played: number; track_count: number };

export type HistoryOverview = {
  /** False when every counter on the device is still zero. */
  any_plays: boolean;
  tracks_total: number;
  tracks_played: number;
  total_plays: number;
  /** Reconstructed from last-played times, newest first. */
  sessions: PlaySession[];
  gap_minutes: number;
  most_played: PlayEntry[];
  never_played: number[];
  cases: PlayedCollection[];
  mixes: PlayedCollection[];
};

/** Play counters, reconstructed sessions and rankings. `gapMinutes` ends a session (default 240). */
export const historyOverview = (root: string, gapMinutes?: number) =>
  invoke<HistoryOverview>("history_overview", { root, gapMinutes: gapMinutes ?? null });

/** Creates a case from the tracks, in order, and returns its id. Writes to the device library. */
export const historySaveAsCase = (root: string, name: string, trackIds: number[]) =>
  invoke<number>("history_save_as_case", { root, name, trackIds });
