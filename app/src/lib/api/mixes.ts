// Mix commands (src-tauri/src/commands/mixes.rs) and the editor's document model.
//
// THE MODEL IN ONE PAGE (read this before building on it)
// =======================================================
// `MixDoc` is a lossless, id-addressed view of the mix document the Tonium Editor and the
// Pacemaker read (notes/mix-format.md). Opening a mix and saving it unchanged writes the
// identical bytes; everything the editor does not understand travels as a "pass-through" field
// that you send back untouched. Rust mirror: commands/mixes/doc.rs (keep the two in step).
//
// Time. Everything is in SAMPLES at 44 100 Hz (`SAMPLE_RATE`); `*Sec` fields are derived
// conveniences (samples / 44100), recomputed by the backend and by `refresh()` in
// state/mixedit.ts, and ignored on save. Two axes exist:
//   - TIMELINE: the mix. Clip `start`, `visibleStart/End`, transition `start`.
//   - CLIP-LOCAL: the clip's own stretched audio, counted from the start of the audio FILE.
//     The timeline position of local `p` is `clip.start + p`. Trims, the tempo / key / gain
//     nodes and every process (tool, loop, ignore, reverse) of a clip live on this axis, so
//     they move with the clip.
// A clip's audible part is local [trimStart, length - trimEnd] = timeline [visibleStart,
// visibleEnd]. `trimStart` is the cue: the Editor's demo has clips whose first 9.9 M samples are
// hidden. So `start` is where the file's first sample would be, not where you hear the clip
// begin; use `visibleStart`/`visibleEnd` for what the user sees and `start` for what is saved.
//
// Tempo. The format has no global tempo. A clip stores the track's analysed `bpm` and a tempo
// curve of `speed` ratios; its effective tempo at local `p` is `bpm * speed(p)`. The curve is
// piecewise linear between nodes, flat outside them (hypothesis; constant curves are the only
// ones seen). With S(p) = integral of speed from 0 to p, S maps clip-local samples to SOURCE
// samples (the unstretched file at 44.1 kHz); `TempoMap` in state/mixedit.ts is the exact port
// of pm_formats::mix::TempoMap. `MixDoc.tempoBpm` is a UI-side reference tempo (derived on open
// as the first clip's effective tempo); it is not saved.
//
// Beats. `clip.track.grid` holds the `.str` beat frames (frames of the audio file at
// `grid.sourceRate`). A beat at frame f is at source f*44100/sourceRate, clip-local
// `localAt(that)`, timeline `clip.start + local`. `clipBeats(clip)` does it. Bars assume 4/4;
// `firstDownbeat` indexes the beat that starts bar 1.
//
// Ids. Every clip, process and transition has a stable `id` (their SessionEventId: 32 lowercase
// hex, or `auto-N` for events that had none). Selection, undo and the loop `parent` link use ids,
// never indexes. New items need a fresh id: `newId()` in state/mixedit.ts.
//
// Channels. `channel` is the deck, 0 or 1. Clips that overlap in time must be on different
// channels, unless a transition covers the overlap. The Editor alternates channels.
//
// What is NOT saved: `tempoBpm`, `meta` (the library row; edit it with `mixSetMeta`), `clip.track`
// (library facts, beat grid), `Between` links (re-derived from geometry on open), every `*Sec`
// field and the `visible*` fields.
import { invoke } from "@tauri-apps/api/core";

/** Engine sample rate; every position in a `MixDoc` is a sample count at this rate. */
export const SAMPLE_RATE = 44100;

/** One mix as listed (the same shape as a case in the library listing). */
export type MixSummary = { id: number; name: string; play_time_secs: number; track_count: number };

/** The mixes on the volume. The mix editor reads this instead of `LibraryView.mixes`. */
export const mixList = (root: string) => invoke<MixSummary[]>("mix_list", { root });

// ---------------------------------------------------------------------------------------------
// Document model
// ---------------------------------------------------------------------------------------------

/** A time range (the Editor's PositionedEvent). Whether it is on the timeline or clip-local
 * depends on its owner (clips and transitions: timeline; processes: clip-local). */
export type Span = {
  /** First sample of the range. */
  start: number;
  /** Length in samples (> 0). */
  length: number;
  /** Samples hidden at the left edge (LOffset). Clips: the cue. Always 0 on tools. */
  trimStart: number;
  /** Samples hidden at the right edge (ROffset). */
  trimEnd: number;
  /** LimitSize: true when the range is bounded by the audio file (a clip's own event). */
  limitSize: boolean;
  /** Derived: start / 44100. */
  startSec: number;
  /** Derived: length / 44100. */
  lengthSec: number;
};

/** Automation node. `pos`: samples from the start of the owning event (a tool, or the clip for
 * `gain`); `value`: the parameter's real value in its own unit (see `TOOL_TYPES` in state/mixedit.ts). Nodes are in
 * non-decreasing `pos`; joined linearly. A two-node envelope's last node sits at `length - 1`. */
export type EnvNode = { pos: number; value: number };

/** Tempo curve node: from clip-local sample `pos` on, play at `speed` x the track's tempo
 * (1.0242 = 2.4 % faster). */
export type TempoNode = { pos: number; speed: number };

/** Key curve node: pitch shift in cents (100 = 1 semitone) from clip-local sample `pos`.
 * `flags` is pass-through (always 1). */
export type KeyNode = { pos: number; cents: number; flags: number };

/** Crossfade curve point: `x` 0..1 across the transition (0 = its start), `value` 0..1 is the
 * fader (0 = only the earlier clip, 1 = only the later one). */
export type CurveNode = { x: number; value: number };

/** Effect class (and its extra members) of an `EffectDoc`. Pass-through, except when you create
 * a tool: use `newTool()`, which builds the right class. */
export type EffectClass =
  | { class: "fx"; active: boolean }
  | { class: "filter" }
  | { class: "crush" }
  | { class: "delay"; delayType: number; active: boolean; syncAgainstBeat: boolean }
  | { class: "eq" }
  | { class: "peq" }
  | { class: "volume" }
  | { class: "trans"; startBpm: number; active: boolean };

export type EffectDoc = {
  /** "Reverb" | "FILTER" | "Crush" | "Echo" | "Roll" | "EQ" | "Wah" | "Volume" | "Trans" | ...
   * It prefixes the keys of the tool's `params`. */
  name: string;
  bypassed: boolean;
} & EffectClass;

/** A tool on a clip's tool lane: one effect over a range, with one envelope per parameter. */
export type ToolDoc = Span & {
  kind: "tool";
  id: string;
  /** Only present when the document had no usable SessionEventId (see `Clip.id`). Pass-through. */
  legacySessionId?: string;
  /** Derived: name of the first effect ("Reverb", "EQ", ...). Ignored on save. */
  toolType: string;
  /** Pass-through: "ToolProcess". */
  name: string;
  /** Muted ("Mute Tool"). */
  bypassed: boolean;
  effects: EffectDoc[];
  /** Envelope per parameter, keyed `<EffectName>-<Parameter>`, e.g. `Reverb-Wet/Dry`, `EQ-Lo`,
   * `Echo-Time ` (Echo/Roll really have a key with a trailing space). All of an effect's keys
   * must be present (`TOOL_TYPES` in state/mixedit.ts); nodes are relative to the tool's start. */
  params: Record<string, EnvNode[]>;
};

/** LOOP region: repeats a slice. Semantics of `iterationLength`/`offsets` follow the Editor's
 * LoopAudioProcess; the demo mix only shows a chain of two (the second continues the first). */
export type LoopDoc = Span & {
  kind: "loop";
  id: string;
  legacySessionId?: string;
  name: string;
  bypassed: boolean;
  /** Length of one iteration in samples. */
  iterationLength: number;
  iterations: number;
  reLoop: boolean;
  /** Length mode of the loop menu. */
  mode: boolean;
  /** Id of the EARLIER loop of the same clip this one continues, or null. */
  parent: string | null;
  /** Pass-through: per-iteration offsets. */
  offsets: { iteration: number; a: number; offset: number }[];
};

/** IGNORE region: skipped audio. No sample exists, layout from the Editor's writer. */
export type IgnoreDoc = Span & {
  kind: "ignore";
  id: string;
  legacySessionId?: string;
  name: string;
  bypassed: boolean;
  startPos: number;
};

/** REVERSE region: played backwards. No sample exists, layout from the Editor's writer. */
export type ReverseDoc = Span & { kind: "reverse"; id: string; legacySessionId?: string; name: string; bypassed: boolean };

/** An item of a clip's process list (document order is preserved on save). Clip-local ranges. */
export type ProcessDoc = ToolDoc | LoopDoc | IgnoreDoc | ReverseDoc;

/** Beat grid of a track (its `.str`). Frames of the audio file at `sourceRate`. */
export type BeatGrid = {
  sourceRate: number;
  beatStartOffset: number;
  /** Frame of every beat, ascending. */
  beatFrames: number[];
  /** Local tempo at each beat (same length). */
  beatBpm: number[];
  /** Index into `beatFrames` of the first downbeat (bar 1), or null when not analysed. */
  firstDownbeat: number | null;
};

/** Library facts about a clip's track. Read-only, filled by `mixOpen` / `mixTrackInfo`. */
export type TrackInfo = {
  trackId: number;
  /** The track is in the library AND its audio file is on the volume. */
  available: boolean;
  /** Why not: "the track was removed from the library", "the audio file is missing from the volume", ... */
  problem: string | null;
  title: string;
  artist: string;
  album: string;
  genre: string;
  key: string;
  /** The library's tempo for the track. */
  bpm: number;
  /** Whole seconds, from the tags (so a clip's real length can differ by under a second). */
  durationSecs: number;
  sourceRate: number;
  /** `/pmdb_tracks/..` location: resolve the AUDIO from this (or `trackId`), not from `Clip.filePath`. */
  location: string;
  grid: BeatGrid | null;
  /** Why there is no grid ("no .str file", "empty .str file", ...). */
  gridProblem: string | null;
};

/** One track on the timeline. Positions: `start` timeline, everything else clip-local (see the
 * header). Fields marked "pass-through" must be sent back unchanged. */
export type Clip = Span & {
  /** Stable id (the clip's SessionEventId, or `auto-N`). Unique across the whole document. */
  id: string;
  legacySessionId?: string;
  /** Library track id. NEGATIVE = the track was removed from the library (the clip stays, cannot be played). */
  trackId: number;
  /** Deck 0 or 1. */
  channel: number;
  /** Derived: start + trimStart, and start + length - trimEnd: the audible range on the timeline. */
  visibleStart: number;
  visibleEnd: number;
  visibleStartSec: number;
  visibleEndSec: number;
  /** The track's analysed tempo as stored in the mix. Effective tempo at local p: bpm * speed(p). */
  bpm: number;
  /** Tempo curve (the Editor's PitchEnvelope). At least one node. */
  tempo: TempoNode[];
  /** NonInterpolated: pitch mode (tempo change also changes pitch) instead of time-stretch. */
  pitchMode: boolean;
  /** Key curve (KeyEnvelope). */
  key: KeyNode[];
  /** Gain curve in dB (0 = unity; -5..0 seen); nodes in clip-local samples. */
  gain: EnvNode[];
  /** Tools, loops, ignore and reverse regions in document order. Ranges are clip-local. */
  processes: ProcessDoc[];
  // pass-through
  filePath: string;
  offset: number;
  clipName: string;
  clipBypassed: boolean;
  toniumId: string;
  otherIds: Record<string, string>;
  fuzzyId: { artist: string; album: string; title: string; genre: string };
  volume: VolumeShell;
  /** Library facts and the beat grid; null until filled. Ignored on save. */
  track: TrackInfo | null;
};

/** Pass-through shell of a clip's volume envelope; the editable part is `Clip.gain`. */
export type VolumeShell = Span & {
  name: string;
  bypassed: boolean;
  effects: EffectDoc[];
  sessionEventId: string;
  otherParams: Record<string, EnvNode[]>;
  gainKeyPresent: boolean;
};

/** A crossfade over the overlap of two clips (timeline range). The document does not say which
 * clips it joins; `between` is derived from geometry on open (and set by `mixAutoTransition`). */
export type TransitionDoc = Span & {
  id: string;
  legacySessionId?: string;
  name: string;
  bypassed: boolean;
  /** The transition's beat-match flag. */
  beatMatch: boolean;
  /** 1 = fade after 1/2, 3 = fade after 3/4 (the only values seen). */
  crossfadeType: number;
  curve: CurveNode[];
  /** Derived, not saved: the earlier and the later clip. */
  between: { from: string; to: string } | null;
};

/** The library row of the mix. Only genre/year/rating/producer/remixer/comments are written
 * (by `mixSetMeta`); the rest is read-only. Dates are Unix milliseconds. */
export type MixMeta = {
  genre: string;
  year: number;
  rating: number;
  producer: string;
  remixer: string;
  comments: string;
  dateCreated: number;
  dateModified: number;
  timesPlayed: number;
  lastPlayed: number;
  playTimeSecs: number;
  creatorId: string;
};

/** The whole mix as the editor sees it. This is also what the audio engine receives
 * (`renderMixPreview`, `exportMixAudio`): Rust converts it with `MixDoc::to_mix`. */
export type MixDoc = {
  /** The library's mix id. */
  id: number;
  /** DocumentName; saving renames the mix (unique, 1..255 chars). */
  name: string;
  /** Document-level Genre / Artist (not the library row's genre; see `meta`). */
  genre: string;
  artist: string;
  /** Pass-through. */
  deviceInfo: string;
  /** Always 44100. */
  sampleRate: number;
  /** Reference tempo (BPM) for beat-matching and the ruler grid. Not saved. */
  tempoBpm: number | null;
  /** In document order (the order of `mixtracks`); sort by `visibleStart` for display. */
  clips: Clip[];
  transitions: TransitionDoc[];
  /** Derived: end of the last audible clip, samples and seconds. */
  length: number;
  lengthSec: number;
  /** The library row; null for a document not read from the library. Ignored by `mixSave`. */
  meta: MixMeta | null;
};

// ---------------------------------------------------------------------------------------------
// Suggestions
// ---------------------------------------------------------------------------------------------

/** Result of `mixBeatmatch`: replace the clip's `tempo`, `start`, `length`, `trimStart`, `trimEnd`
 * with these (use `applyBeatmatch()` in state/mixedit.ts: it also moves the clip's tools, loops
 * and key / gain nodes with the audio). */
export type BeatmatchSuggestion = {
  clipId: string;
  referenceClipId: string;
  tempo: TempoNode[];
  start: number;
  length: number;
  trimStart: number;
  trimEnd: number;
  /** Tempo the clip now plays at: the reference clip's effective tempo where this clip enters. */
  targetBpm: number;
  speed: number;
  /** How far the clip's audible start moved on the timeline, in samples (positive = later). */
  shift: number;
  /** Beat index of the clip's grid pinned to beat `referenceBeat` of the reference clip's grid. */
  anchorBeat: number;
  referenceBeat: number;
  /** Bars line up too (needs a first downbeat in both grids; 4/4 assumed). */
  barAligned: boolean;
  /** The reference clip is still audible where this clip enters. */
  overlaps: boolean;
  /** Things to tell the user (large stretch, no downbeat, big move, ...). */
  notes: string[];
};

/** A tool to add to a clip as part of a suggestion. */
export type ToolSuggestion = {
  clipId: string;
  tool: ToolDoc;
  /** Existing EQ tools of that clip overlapping it; the user decides (replace / skip). */
  conflicts: string[];
};

/** Result of `mixAutoTransition`: add `transition`, and the `tools` to their clips
 * (`applyTransition()`). */
export type TransitionSuggestion = {
  transition: TransitionDoc;
  /** Bass swap: EQ tools cutting the incoming clip's lows until the middle of the overlap and
   * the outgoing clip's lows after it. EQ value semantics are a hypothesis (Lo -100 = kill). */
  tools: ToolSuggestion[];
  /** Overlap length in bars (4 beats) at the incoming clip's tempo. */
  bars: number;
  /** Samples in one beat of the incoming clip at the transition start. */
  beat: number;
  /** How many bars both clips are audible together from the transition start: the longest the
   * transition could be. */
  barsAvailable: number;
  notes: string[];
};

/** What a save reports. */
export type MixSaved = { id: number; name: string; playTimeSecs: number; trackCount: number; dateModified: number };

// ---------------------------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------------------------

/** Opens a mix: clips, tools, library facts and beat grids of every track, the library row. */
export const mixOpen = (root: string, id: number) => invoke<MixDoc>("mix_open", { root, id });

/** Creates an empty mix; returns its id. Errors if the name is empty, over 255 characters or taken. */
export const mixNew = (root: string, name: string) => invoke<number>("mix_new", { root, name });

/** Validates and writes the document (blob, mixtracks, play time, preview markers, date_modified,
 * and the name). Refuses with a message naming every problem when: a track is not in the library,
 * clips overlap on one channel without a transition, a tool lacks one of its parameter envelopes,
 * envelope nodes are out of order, a range is empty or runs past its clip, two tools of one kind
 * overlap, ids repeat, the name is invalid or taken. An unchanged document writes identical bytes. */
export const mixSave = (root: string, id: number, doc: MixDoc) => invoke<MixSaved>("mix_save", { root, id, doc });

/** The problems `mixSave` would refuse the document for (empty = it would save). Writes nothing. */
export const mixValidate = (root: string, doc: MixDoc) => invoke<string[]>("mix_validate", { root, doc });

/** Renames a mix. */
export const mixRename = (root: string, id: number, name: string) => invoke<void>("mix_rename", { root, id, name });

/** Deletes a mix for good. The caller must have asked the user, naming the mix. True if it existed. */
export const mixDelete = (root: string, id: number) => invoke<boolean>("mix_delete", { root, id });

/** Writes genre, year (0..9999), rating (0..5), producer, remixer, comments of the library row. */
export const mixSetMeta = (root: string, id: number, meta: MixMeta) => invoke<void>("mix_set_meta", { root, id, meta });

/** Imports a `.pmk` file as a new mix; returns its id. Its tracks are matched to the library by
 * title + artist; it is refused, listing them, if some are missing. A taken name gets " (2)". */
export const mixImportPmk = (root: string, path: string) => invoke<number>("mix_import_pmk", { root, path });

/** Writes the SAVED mix as a `.pmk` file (overwrites; ask the user first). */
export const mixExportPmk = (root: string, id: number, path: string) => invoke<void>("mix_export_pmk", { root, id, path });

/** Library facts + beat grids for tracks, in the order asked (for adding tracks, or refreshing). */
export const mixTrackInfo = (root: string, trackIds: number[]) => invoke<TrackInfo[]>("mix_track_info", { root, trackIds });

/** A new clip for a library track: channel 0 at timeline 0, speed set for `tempoBpm` when given,
 * with the track's grid. Position it and add it (`addClip()` in state/mixedit.ts). */
export const mixMakeClip = (root: string, trackId: number, tempoBpm: number | null) =>
  invoke<Clip>("mix_make_clip", { root, trackId, tempoBpm });

/** Suggests how to align a clip to the clip before it (tempo + start). Changes nothing. */
export const mixBeatmatch = (root: string, doc: MixDoc, clipId: string) =>
  invoke<BeatmatchSuggestion>("mix_beatmatch", { root, doc, clipId });

/** Suggests the transition between two clips: `bars` long (default 16; the largest that fits
 * when the clips share less audio), a linear crossfade and a bass swap. `state/transitionRecipes.ts`
 * reshapes that into the other recipes. Changes nothing. */
export const mixAutoTransition = (root: string, doc: MixDoc, fromClipId: string, toClipId: string, bars: number | null = null) =>
  invoke<TransitionSuggestion>("mix_auto_transition", { root, doc, fromClipId, toClipId, bars });

// ---------------------------------------------------------------------------------------------
// Tool catalogue types (the catalogue itself, TOOL_TYPES, is in state/mixedit.ts)
// ---------------------------------------------------------------------------------------------

/** One automated parameter of a tool. Ranges are what the Editor's demo mix contains, not
 * proven limits. `key` is the full envelope key (`EQ-Lo`). */
export type ToolParam = { key: string; label: string; min: number; max: number; unit: string; flat: number };

/** What the tool menu offers. `name` is the effect name. */
export type ToolType = { name: string; label: string; group: "fx" | "colour" | "eq"; params: ToolParam[] };
