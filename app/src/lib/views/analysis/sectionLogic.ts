// Pure logic of the sections feature (phrase detection): wording, settings limits, lookups and the
// snapping function the mix editor calls. No store or component imports (types only) so it can be
// tested with plain node:
// `node --experimental-strip-types src/lib/views/analysis/sectionLogic.test.mjs`.
import type { SectionKind, SectionOut, SectionsSettings, SectionsState, TrackSections } from "../../api/analysis";

export const DEFAULT_SETTINGS: SectionsSettings = {
  sensitivity: 3,
  min_section_bars: 8,
  phrase_bars: 0,
  origin: "first_downbeat",
  snap_tolerance_bars: 2,
  min_bars: 32,
  threads: 2,
  show_markers: true,
};

/** The ranges the backend enforces (`Settings::validate` in commands/analysis/sections.rs). */
export const LIMITS = {
  sensitivity: { min: 1, max: 5 },
  min_section_bars: { min: 4, max: 32 },
  snap_tolerance_bars: { min: 0, max: 8 },
  min_bars: { min: 16, max: 512 },
  threads: { min: 1, max: 8 },
} as const;

export type NumericSetting = keyof typeof LIMITS;

/** Phrase lengths the setting accepts; 0 is "choose from each track". */
export const PHRASE_CHOICES = [0, 4, 8, 16, 32] as const;

/** `value` forced into the setting's range (whole numbers); NaN becomes the default. */
export function clampSetting(key: NumericSetting, value: number): number {
  if (!Number.isFinite(value)) return DEFAULT_SETTINGS[key];
  const { min, max } = LIMITS[key];
  return Math.min(max, Math.max(min, Math.round(value)));
}

export function sameSettings(a: SectionsSettings, b: SectionsSettings): boolean {
  return (Object.keys(DEFAULT_SETTINGS) as (keyof SectionsSettings)[]).every((k) => a[k] === b[k]);
}

export const SENSITIVITY_NAMES = ["", "Lowest", "Low", "Normal", "High", "Highest"] as const;

export const KIND_LABEL: Record<SectionKind, string> = {
  intro: "Intro",
  build: "Build",
  drop: "Drop",
  breakdown: "Breakdown",
  outro: "Outro",
  section: "Section",
};

/** Kinds in the order a legend lists them. */
export const KINDS: SectionKind[] = ["intro", "build", "drop", "breakdown", "outro", "section"];

/** The CSS colour of a kind: a token from tokens.css, never a raw colour. */
export const kindColor = (kind: SectionKind) => `var(--sec-${kind})`;

export function entriesById(entries: readonly TrackSections[]): Map<number, TrackSections> {
  return new Map(entries.map((e) => [e.track_id, e]));
}

/** Marker opacity from a confidence: a weak guess is faint but never invisible. */
export const confidenceOpacity = (c: number) => Math.round((0.35 + 0.65 * Math.min(1, Math.max(0, c))) * 100) / 100;

export const percent = (c: number) => `${Math.round(Math.min(1, Math.max(0, c)) * 100)}%`;

/** `m:ss` */
export function clock(secs: number): string {
  const s = Math.max(0, Math.round(secs));
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
}

/** Total length covered by a track's sections (the end of the last one). */
export const coveredSecs = (sections: readonly SectionOut[]) => (sections.length ? sections[sections.length - 1].end_secs : 0);

/** The section holding `secs`, or null outside them. Ends are exclusive, except the last. */
export function sectionAt(sections: readonly SectionOut[], secs: number): SectionOut | null {
  for (let i = 0; i < sections.length; i++) {
    const s = sections[i];
    if (secs >= s.start_secs && (secs < s.end_secs || (i === sections.length - 1 && secs <= s.end_secs))) return s;
  }
  return null;
}

/** "Drop, 0:30 to 1:30, bars 17 to 48, confidence 92%" */
export function sectionTitle(s: SectionOut): string {
  return `${KIND_LABEL[s.kind]}, ${clock(s.start_secs)} to ${clock(s.end_secs)}, bars ${s.start_bar + 1} to ${s.end_bar}, confidence ${percent(s.confidence)}`;
}

/** One line for a tooltip over a whole strip of markers. */
export function summaryTitle(sections: readonly SectionOut[]): string {
  return `Sections (estimated): ${sections.map((s) => `${KIND_LABEL[s.kind].toLowerCase()} ${clock(s.start_secs)}`).join(", ")}`;
}

/** What the chip under the grid editor's overview says about the phrase grid. */
export function phraseText(entry: TrackSections): string {
  const marked = entry.sections.slice(1);
  if (entry.phrase_bars === 0) return marked.length ? "Boundaries sit where the music changes: no phrase grid fits this track." : "";
  const on = marked.filter((s) => s.on_phrase).length;
  return `Boundaries snap to a ${entry.phrase_bars}-bar phrase grid (${on} of ${marked.length} on it).`;
}

// ---- snapping (the mix editor's entry point) -------------------------------------------------

/** A boundary between two sections: where one ends and the next begins. */
export type Boundary = {
  secs: number;
  /** Bar index from the first downbeat of the grid. */
  bar: number;
  before: SectionKind;
  after: SectionKind;
  /** How strong the boundary is, 0 to 1. */
  confidence: number;
  /** It sits on the phrase grid. */
  on_phrase: boolean;
};

export type SnapOptions = {
  /** Farthest to move, in seconds. Default 8. */
  maxSecs?: number;
  /** Only boundaries where one of these kinds begins ("snap to a drop"). */
  into?: readonly SectionKind[];
  /** Only boundaries where one of these kinds ends. */
  from?: readonly SectionKind[];
  /** Ignore boundaries weaker than this (0 to 1). Default 0. */
  minConfidence?: number;
  /** Only boundaries on the phrase grid. Default false. */
  onPhraseOnly?: boolean;
};

/** The boundaries of a track (the start of every section but the first), ascending, optionally
 * filtered. The start and end of the track are not boundaries. */
export function boundariesOf(sections: readonly SectionOut[], opts: SnapOptions = {}): Boundary[] {
  const out: Boundary[] = [];
  for (let i = 1; i < sections.length; i++) {
    const a = sections[i - 1];
    const b = sections[i];
    if (opts.into && !opts.into.includes(b.kind)) continue;
    if (opts.from && !opts.from.includes(a.kind)) continue;
    if ((opts.minConfidence ?? 0) > b.start_confidence) continue;
    if (opts.onPhraseOnly && !b.on_phrase) continue;
    out.push({ secs: b.start_secs, bar: b.start_bar, before: a.kind, after: b.kind, confidence: b.start_confidence, on_phrase: b.on_phrase });
  }
  return out;
}

export type Snap = { secs: number; distance: number; boundary: Boundary };

/** The boundary nearest to `secs` within `maxSecs`, or null: where a clip start, an in or out
 * point or a cue would land to line up with the track's structure. A tie goes to the earlier
 * boundary. Same rule as `snap_secs` in crates/pm-analysis/src/sections.rs. */
export function snapToBoundary(sections: readonly SectionOut[], secs: number, opts: SnapOptions = {}): Snap | null {
  if (!Number.isFinite(secs)) return null;
  const reach = opts.maxSecs ?? 8;
  let best: Snap | null = null;
  for (const b of boundariesOf(sections, opts)) {
    const distance = Math.abs(b.secs - secs);
    if (distance <= reach && (best === null || distance < best.distance)) best = { secs: b.secs, distance, boundary: b };
  }
  return best;
}

/** The next (`dir` 1) or previous (`dir` -1) boundary strictly after / before `secs`, or null:
 * for "jump to the next drop" style controls. */
export function neighbourBoundary(sections: readonly SectionOut[], secs: number, dir: 1 | -1, opts: SnapOptions = {}): Boundary | null {
  const all = boundariesOf(sections, opts);
  if (dir === 1) return all.find((b) => b.secs > secs) ?? null;
  for (let i = all.length - 1; i >= 0; i--) if (all[i].secs < secs) return all[i];
  return null;
}

// ---- wording ---------------------------------------------------------------------------------

/** Tracks a default run would analyse: never analysed, or the file has changed since. A track
 * whose grid was edited is found by the plan (it looks at the card), not counted here. */
export const toAnalyse = (s: SectionsState): number => s.none + s.stale;

const plural = (n: number, one: string, many = `${one}s`) => `${n} ${n === 1 ? one : many}`;

/** The sentence under the panel's title. */
export function coverageText(s: SectionsState): string {
  if (s.total === 0) return "This library has no tracks.";
  const parts = [`${s.ready} of ${plural(s.total, "track")} ${s.ready === 1 ? "has" : "have"} sections.`];
  if (s.none > 0) parts.push(`${s.none} not analysed yet.`);
  if (s.stale > 0) parts.push(`${plural(s.stale, "file")} changed since ${s.stale === 1 ? "it was" : "they were"} analysed.`);
  if (s.flat > 0) parts.push(`${s.flat} analysed but without clear sections at this sensitivity.`);
  if (s.skipped > 0) parts.push(`${s.skipped} with nothing to show (see below).`);
  return parts.join(" ");
}

/** A hint for fixing a skip, for the reasons the user can act on. */
export function skipHint(reason: string): string {
  if (reason.includes("first downbeat")) return "Set the first downbeat in the Grid editor, then analyse again.";
  if (reason.includes("beat grid")) return "Analyse the track so it has a beat grid.";
  if (reason.startsWith("fewer than")) return "Lower the shortest track length below to include them.";
  if (reason.includes("too short")) return "Too few bars to find sections in.";
  return "";
}

/** Title of the confirmation of a run. */
export function runTitle(count: number, root: string, recompute: boolean): string {
  return `${recompute ? "Analyse again" : "Detect sections in"} ${plural(count, "track")} on ${root} ?`;
}

export const RUN_DESCRIPTION =
  "Each track is decoded and measured bar by bar, using the bars of its beat grid, for level in three bands and how busy it is. Nothing is written to the device or to the audio files: the results are kept by this app, per library, and the sections do not appear on the device. Section detection is an estimate from the audio: it works best on dance music with a steady grid and clear changes, and it can be wrong, most often on builds. Check markers by ear.";

export const LIMITATIONS =
  "How it works: bars come from the beat grid (a wrong grid or first downbeat gives wrong sections); the level and busyness of each bar are compared with the rest of the track; boundaries are placed where bars before and after a point differ most, and moved onto the 8, 16 or 32 bar phrase grid when one is close. Labels come from loudness relative to the rest of the track. It is a heuristic, checked on synthetic audio, not on a labelled set of real tracks. Expect it to struggle with tracks that change little, tempo changes and half-time sections; builds are the least reliable label. Confidence is relative to the track, not a probability.";
