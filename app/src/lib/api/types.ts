// Shared wire types: the shapes the Rust commands return and the events they emit. Integration-
// owned. An area that needs a type only it uses declares it in its own api/<area>.ts instead.

// Track, Collection and LibraryView are read only on purpose: the open library is a set of shallow
// signals (see session.svelte.ts), so a field written in place would never reach a screen. Change
// a track through `session.patchTracks`, the case and mix lists through `setCases` / `setMixes`.
export type Track = {
  readonly track_id: number;
  readonly title: string;
  readonly artist: string;
  readonly album: string;
  readonly genre: string;
  readonly bpm: number;
  readonly key: string;
  readonly play_time_secs: number;
  readonly rating: number;
  readonly location: string;
  readonly file_size: number;
  readonly format: string;
  readonly analysed: boolean;
  readonly times_played: number;
  /** Unix milliseconds of the last play; -1 = never. */
  readonly last_played: number;
  /** Release year; 0 = not set. */
  readonly year: number;
  readonly label: string;
  readonly remixer: string;
  readonly composer: string;
  readonly comments: string;
  /** Unix milliseconds the track was added; -1 = unknown. */
  readonly date_added: number;
  /** Kilobits per second; 0 = unknown. */
  readonly bit_rate: number;
};

export type Collection = { readonly id: number; readonly name: string; readonly play_time_secs: number; readonly track_count: number };

export type NeedsAnalysis = { readonly track_id: number; readonly reason: string };

export type LibraryView = {
  readonly root: string;
  readonly problems: string[];
  readonly tracks: Track[];
  readonly mixes: Collection[];
  readonly cases: Collection[];
  readonly needs_analysis: NeedsAnalysis[];
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
