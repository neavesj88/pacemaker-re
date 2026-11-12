// Shared wire types: the shapes the Rust commands return and the events they emit. Integration-
// owned. An area that needs a type only it uses declares it in its own api/<area>.ts instead.

export type Track = {
  track_id: number;
  title: string;
  artist: string;
  album: string;
  genre: string;
  bpm: number;
  key: string;
  play_time_secs: number;
  rating: number;
  location: string;
  file_size: number;
  format: string;
  analysed: boolean;
  times_played: number;
  /** Unix milliseconds of the last play; -1 = never. */
  last_played: number;
  /** Release year; 0 = not set. */
  year: number;
  label: string;
  remixer: string;
  composer: string;
  comments: string;
  /** Unix milliseconds the track was added; -1 = unknown. */
  date_added: number;
  /** Kilobits per second; 0 = unknown. */
  bit_rate: number;
};

export type Collection = { id: number; name: string; play_time_secs: number; track_count: number };

export type NeedsAnalysis = { track_id: number; reason: string };

export type LibraryView = {
  root: string;
  problems: string[];
  tracks: Track[];
  mixes: Collection[];
  cases: Collection[];
  needs_analysis: NeedsAnalysis[];
};

/** What kind of job: "add", "reanalyse", or any string a new area picks. */
export type JobKind = string;

export type ProgressEvent = {
  job: JobKind;
  label: string;
  done: number;
  total: number;
  /** Free-form per kind; "failed" means this item failed and is listed in the strip. */
  status: string;
  bpm: number | null;
  detail: string | null;
  error: string | null;
};

export type DoneEvent = {
  job: JobKind;
  ok: number;
  failed: number;
  /** The finished line, already worded by the backend. */
  summary: string;
  error: string | null;
};
