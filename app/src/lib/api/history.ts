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

// ---- play log (src-tauri/src/commands/history/logcmds.rs) -------------------------------------
// An app-side record of how the device's play counters changed. It lives in the app config dir,
// never on the device, and is not visible on the Pacemaker.

export type PlayLogSettings = {
  /** Take a snapshot each time a library is read. */
  enabled: boolean;
  /** Days of changes kept; 0 keeps everything. */
  retention_days: number;
  /** A pause longer than this many minutes ends a session. */
  session_gap_minutes: number;
  /** Cap on the changes kept in the file whatever the retention says; the oldest fold into the starting point past it. */
  max_events: number;
  /** Percent of the played tracks the log knows that must still be in the library, or it is taken for a different card. 0 never refuses. */
  identity_min_present_pct: number;
  /** Ask before starting a new log for a card that matches the log of another drive. */
  ask_before_new_log: boolean;
};

export type RecordStatus = "off" | "started" | "unchanged" | "changed" | "mismatch" | "adopted" | "candidate" | "recovered";
export type RecordReport = {
  enabled: boolean;
  status: RecordStatus;
  tracks: number;
  pruned: number;
  detail: string | null;
};

/** One derived play. `at` is the device clock (Unix ms) of the most recent of `plays`. */
export type LogHit = {
  track_id: number;
  at: number;
  plays: number;
  /** Several plays share this one time, or the time was inferred. */
  approx: boolean;
  /** False: the device gave no usable time, `at` is when the library was read. */
  time_known: boolean;
  /** The counter had gone backwards (reset) just before. */
  reset: boolean;
};
export type LogSession = { start: number; end: number; plays: number; hits: LogHit[] };
export type LogRank = { track_id: number; plays: number; windows: number; first: number; last: number };

/** The log of another drive that matches most of this library's tracks. */
export type LogCandidate = {
  /** File name in the app's config folder; pass it to `playlogAdoptLog`. */
  file: string;
  /** Where that log last saw its library; "" when not known. */
  root: string;
  started_at: number;
  last_change_at: number;
  played_tracks: number;
  present_tracks: number;
};

export type PlayLogOverview = {
  enabled: boolean;
  has_log: boolean;
  /** The saved file could not be used. */
  problem: string | null;
  /** The problem is damage, so the next snapshot moves the file aside; otherwise (unreadable just now, or from a newer app) it is left alone. */
  problem_moves_aside: boolean;
  /** This library is not the one the log was made from. */
  mismatch: boolean;
  /** The drive or path the log last saw its library at; the log is tied to it. "" when not known. */
  log_root: string;
  /** While there is no log: logs of other drives that match this library. */
  candidates: LogCandidate[];
  started_at: number;
  last_change_at: number;
  events: number;
  file_bytes: number;
  retention_days: number;
  gap_minutes: number;
  first_play_at: number;
  last_play_at: number;
  total_plays: number;
  /** Plays in the period of tracks that are no longer in the library. */
  removed_plays: number;
  sessions_total: number;
  /** Newest first. */
  sessions: LogSession[];
  ranking_total: number;
  /** The most played tracks of the period, cut to the page size. */
  ranking: LogRank[];
  /** Every track of the period's ranking, in rank order (not cut): what "save as case" saves. */
  ranking_ids: number[];
};

export type PlayLogQuery = {
  /** Unix ms bounds of the period, either may be absent. */
  from?: number | null;
  to?: number | null;
  gapMinutes?: number | null;
  sessionLimit?: number;
  rankLimit?: number;
};

export type TrackLog = {
  track_id: number;
  before_plays: number;
  before_last: number;
  logged_plays: number;
  now_plays: number;
  now_last: number;
  /** Newest first. */
  hits: LogHit[];
};

export const playlogSettings = () => invoke<PlayLogSettings>("playlog_settings_get");
export const playlogSetSettings = (settings: PlayLogSettings) => invoke<PlayLogSettings>("playlog_settings_set", { settings });
/** Snapshots the counters into the log. Writes only the app's own file, and only if something changed. */
export const playlogRecord = (root: string) => invoke<RecordReport>("playlog_record", { root });
/** "Keep this log and continue": accepts a library the log refused as a different card. Tracks it lacks are set aside, tracks the log never saw start from their present counters. */
export const playlogAdopt = (root: string) => invoke<RecordReport>("playlog_adopt", { root });
/** Starts a new log for this library although the log of another drive matches it. */
export const playlogStartNew = (root: string) => invoke<RecordReport>("playlog_start_new", { root });
/** Continues the log of another drive here: moves its file (nothing is copied or deleted). */
export const playlogAdoptLog = (root: string, file: string) => invoke<void>("playlog_adopt_log", { root, file });
export const playlogOverview = (root: string, q: PlayLogQuery = {}) =>
  invoke<PlayLogOverview>("playlog_overview", {
    root,
    from: q.from ?? null,
    to: q.to ?? null,
    gapMinutes: q.gapMinutes ?? null,
    sessionLimit: q.sessionLimit ?? null,
    rankLimit: q.rankLimit ?? null,
  });
export const playlogTrack = (root: string, trackId: number) => invoke<TrackLog | null>("playlog_track", { root, trackId });
/** Deletes this library's log (the device and its counters are untouched). */
export const playlogClear = (root: string) => invoke<boolean>("playlog_clear", { root });
