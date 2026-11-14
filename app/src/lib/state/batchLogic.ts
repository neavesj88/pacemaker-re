// Pure logic of the batch grid tools and the suspect-grid flags: reading typed numbers and genre
// windows, the remembered settings, and the wording of the preview, the mixes warning and the
// confirmation. No store or component imports so it runs under plain node: see batchLogic.test.mjs.
import type { BatchRow, GenreWindow, SuspectSettings } from "../api/grid";
import type { LibraryKind } from "../api/types";
import defaults from "./suspectDefaults.json" with { type: "json" };

/** The starting rules. The file is the single source: a Rust test keeps it equal to the backend's own defaults. */
export const DEFAULT_SUSPECT_SETTINGS: SuspectSettings = defaults as SuspectSettings;

export const SUSPECT_STORAGE = "pm.gridSuspects.v1";
export const NUDGE_STORAGE = "pm.gridBatchNudgeMs";

/** Tempo range the device accepts, and the furthest a nudge may move; the backend enforces both. */
export const MIN_BPM = 20;
export const MAX_BPM = 400;
export const MAX_NUDGE_MS = 5000;

/** What the app remembers about the rules: the rules themselves and whether the library is checked
 *  in the background whenever it loads. */
export type StoredSuspect = { rules: SuspectSettings; auto: boolean };

const num = (v: unknown, fallback: number) => (typeof v === "number" && Number.isFinite(v) ? v : fallback);
const bool = (v: unknown, fallback: boolean) => (typeof v === "boolean" ? v : fallback);

/** Rules read back from storage (or anywhere): every field that is missing or not the right kind
 *  takes its default, so an old or damaged value never breaks the tool. Values are not range-checked
 *  here: `validateSettings` does that for what the user types. */
export function normaliseSuspect(raw: unknown): StoredSuspect {
  const d = DEFAULT_SUSPECT_SETTINGS;
  const o = (raw && typeof raw === "object" ? raw : {}) as Record<string, unknown>;
  const r = (o.rules && typeof o.rules === "object" ? o.rules : {}) as Record<string, unknown>;
  const genres = Array.isArray(r.genres)
    ? r.genres
        .filter((g): g is Record<string, unknown> => !!g && typeof g === "object")
        .map((g) => ({ name: typeof g.name === "string" ? g.name : "", min: num(g.min, NaN), max: num(g.max, NaN) }))
        .filter((g) => g.name.trim() !== "" && Number.isFinite(g.min) && Number.isFinite(g.max))
    : d.genres.map((g) => ({ ...g }));
  return {
    auto: bool(o.auto, true),
    rules: {
      tempo_rule: bool(r.tempo_rule, d.tempo_rule),
      fallback_min: num(r.fallback_min, d.fallback_min),
      fallback_max: num(r.fallback_max, d.fallback_max),
      genres,
      quality_rule: bool(r.quality_rule, d.quality_rule),
      min_confidence_pct: num(r.min_confidence_pct, d.min_confidence_pct),
      steady_tolerance_pct: num(r.steady_tolerance_pct, d.steady_tolerance_pct),
      coverage_target_pct: num(r.coverage_target_pct, d.coverage_target_pct),
    },
  };
}

/** The reason the rules cannot be used, or "" when they can. Mirrors `SuspectSettings::validate`. */
export function validateSettings(s: SuspectSettings): string {
  const window = (what: string, lo: number, hi: number) =>
    !Number.isFinite(lo) || !Number.isFinite(hi) || lo < MIN_BPM || hi > MAX_BPM || lo >= hi
      ? `The ${what} tempo window must run from ${MIN_BPM} up to ${MAX_BPM} BPM, with the first number below the second.`
      : "";
  const bad = window("general", s.fallback_min, s.fallback_max);
  if (bad) return bad;
  for (const g of s.genres) {
    if (g.name.trim() === "") return "A genre window needs a name.";
    const e = window(`“${g.name.trim()}”`, g.min, g.max);
    if (e) return e;
  }
  const pct = (what: string, v: number, lo: number, hi: number) =>
    Number.isFinite(v) && v >= lo && v <= hi ? "" : `${what} must be between ${lo} and ${hi} per cent.`;
  return (
    pct("The confidence floor", s.min_confidence_pct, 0, 100) ||
    pct("The steady-gap tolerance", s.steady_tolerance_pct, 0.5, 50) ||
    pct("The coverage target", s.coverage_target_pct, 10, 100)
  );
}

const trimNum = (v: number) => (Math.abs(v - Math.round(v)) < 1e-9 ? String(Math.round(v)) : String(v));

/** One window per line, "house 115-132", to the list; a line that cannot be read is an error naming it. */
export function parseGenreWindows(text: string): { genres: GenreWindow[] } | { error: string } {
  const genres: GenreWindow[] = [];
  const names = new Set<string>();
  for (const [i, line] of text.split(/\r?\n/).entries()) {
    const t = line.trim();
    if (t === "") continue;
    const m = /^(.+?)[\s:=]+(\d+(?:[.,]\d+)?)\s*(?:-|–|—|to)\s*(\d+(?:[.,]\d+)?)$/i.exec(t);
    if (!m) return { error: `Line ${i + 1} (“${t}”) is not a genre and a range, like “house 115-132”.` };
    const name = m[1].trim();
    if (names.has(name.toLowerCase())) return { error: `“${name}” is listed twice.` };
    names.add(name.toLowerCase());
    genres.push({ name, min: Number(m[2].replace(",", ".")), max: Number(m[3].replace(",", ".")) });
  }
  return { genres };
}

export const formatGenreWindows = (genres: GenreWindow[]) => genres.map((g) => `${g.name} ${trimNum(g.min)}-${trimNum(g.max)}`).join("\n");

/** A typed nudge in milliseconds ("-12,5") inside the allowed range, else an error message. */
export function parseNudge(text: string): { ms: number } | { error: string } {
  const t = text.trim().replace(",", ".");
  if (t === "") return { error: "Type how far to move the beat lines, in milliseconds." };
  const n = Number(t);
  if (!Number.isFinite(n)) return { error: `"${text.trim()}" is not a number.` };
  if (n === 0) return { error: "0 ms moves nothing." };
  if (Math.abs(n) > MAX_NUDGE_MS) return { error: `A nudge can be at most ${MAX_NUDGE_MS} ms either way.` };
  return { ms: Math.round(n * 100) / 100 };
}

// ---- the preview ---------------------------------------------------------------------------

export type BatchKind = "nudge" | "halve" | "double" | "suggested" | "reanalyse";

export const KIND_LABEL: Record<BatchKind, string> = {
  nudge: "Nudge",
  halve: "Halve BPM",
  double: "Double BPM",
  suggested: "Suggested fix",
  reanalyse: "Re-analyse",
};

/** What is done to one track, in a word (the row's `op`; a suggested fix is a halve or a double). */
export const OP_LABEL: Record<string, string> = { nudge: "Nudge", halve: "Halve", double: "Double", reanalyse: "Re-analyse", suggested: "Suggested fix" };

export const fmtBpm = (n: number | null) => (n === null ? "?" : n.toFixed(2));

/** `m:ss.mmm` of a position in frames; "?" when unknown. */
export function fmtFrames(frames: number | null, sampleRate: number): string {
  if (frames === null || sampleRate <= 0) return "?";
  const secs = Math.max(0, frames) / sampleRate;
  const m = Math.floor(secs / 60);
  return `${m}:${(secs - m * 60).toFixed(3).padStart(6, "0")}`;
}

/** How far the first beat line moves, in milliseconds (positive = later); null when either end is unknown. */
export function firstShiftMs(r: BatchRow): number | null {
  if (r.before_first === null || r.after_first === null || r.sample_rate <= 0) return null;
  return ((r.after_first - r.before_first) / r.sample_rate) * 1000;
}

export const fmtShift = (ms: number | null) => (ms === null ? "?" : `${ms >= 0 ? "+" : "−"}${Math.abs(ms).toFixed(1)} ms`);

/** Rows the job will change (those with a new grid). */
export const changing = (rows: BatchRow[]) => rows.filter((r) => r.skip === null);

/** The preview's line for a row, in words: what happens to the track and why not. */
export function rowLine(r: BatchRow, willChange = true): string {
  if (r.skip !== null) return `${r.label}: left as it is (${r.skip})`;
  if (!willChange) return `${r.label}: left as it is (not ticked)`;
  const bpm = `${fmtBpm(r.before_bpm)} → ${fmtBpm(r.after_bpm)} BPM`;
  const first = `first beat ${fmtFrames(r.before_first, r.sample_rate)} → ${fmtFrames(r.after_first, r.sample_rate)} (${fmtShift(firstShiftMs(r))})`;
  return `${r.label}: ${(OP_LABEL[r.op] ?? r.op).toLowerCase()}, ${bpm}, ${first}`;
}

export type MixUse = { id: number; name: string; tracks: number };

/** The mixes that use the ticked tracks that will change, each with how many of those tracks it uses
 *  (most first). `unchecked` is true when the lookup failed for some track: staying silent would read
 *  as "no mix is affected". */
export function mixUse(rows: BatchRow[], ticked: ReadonlySet<number>): { mixes: MixUse[]; unchecked: number } {
  const byId = new Map<number, MixUse>();
  let unchecked = 0;
  for (const r of rows) {
    if (r.skip !== null || !ticked.has(r.track_id)) continue;
    for (const m of r.mixes) {
      if (m.id < 0) {
        unchecked++;
        continue;
      }
      const hit = byId.get(m.id);
      if (hit) hit.tracks++;
      else byId.set(m.id, { id: m.id, name: m.name, tracks: 1 });
    }
  }
  const mixes = [...byId.values()].sort((a, b) => b.tracks - a.tracks || a.name.localeCompare(b.name));
  return { mixes, unchecked };
}

/** The sentence the preview and the confirmation carry when mixes use the tracks, or "". */
export function mixesWarning(use: { mixes: MixUse[]; unchecked: number }): string {
  const { mixes, unchecked } = use;
  const parts: string[] = [];
  if (mixes.length > 0) {
    const list = mixes.map((m) => `${m.name} (${m.tracks} track${m.tracks === 1 ? "" : "s"})`).join(", ");
    parts.push(
      `${mixes.length === 1 ? "1 mix uses" : `${mixes.length} mixes use`} some of these tracks: ${list}. A mix clip keeps the tempo the track had when the mix was made, so ${mixes.length === 1 ? "that mix" : "those mixes"} may drift out of step until beatmatched again.`,
    );
  }
  if (unchecked > 0) parts.push(`Could not check which mixes use ${unchecked === 1 ? "one of the tracks" : `${unchecked} of the tracks`}; a mix keeps the tempo its tracks had when it was made.`);
  return parts.join(" ");
}

export type Counts = { change: number; skipped: number; unticked: number };

export function counts(rows: BatchRow[], ticked: ReadonlySet<number>): Counts {
  let change = 0;
  let skipped = 0;
  let unticked = 0;
  for (const r of rows) {
    if (r.skip !== null) skipped++;
    else if (ticked.has(r.track_id)) change++;
    else unticked++;
  }
  return { change, skipped, unticked };
}

/** Identifies what a preview was computed for; a preview whose key differs from the controls' key is stale. */
export const previewKey = (kind: BatchKind, nudgeMs: number, ids: readonly number[]) => `${kind}|${kind === "nudge" ? nudgeMs : ""}|${[...ids].sort((a, b) => a - b).join(",")}`;

/** The sentence every grid change ends with; "off the device" for a Pacemaker, "out of the library" for one on this computer. */
export const bothNote = (kind: LibraryKind = "pacemaker") =>
  `The beat grid (.str) and the BPM of each track are rewritten together, so they always agree. Each track's previous beat grid is first copied ${kind === "computer" ? "out of the library" : "off the device"} (the app's data folder, next to the music.db snapshots), because Restore from a snapshot does not bring beat grids back.`;

/** The confirmation's question: "Halve the BPM of 3 tracks on P:\ ?". */
export function opTitle(kind: BatchKind, n: number, root: string, nudgeMs = 0): string {
  const tracks = `${n} track${n === 1 ? "" : "s"}`;
  switch (kind) {
    case "nudge":
      return `Move the beat lines of ${tracks} by ${fmtShift(nudgeMs)} on ${root} ?`;
    case "halve":
      return `Halve the BPM of ${tracks} on ${root} ?`;
    case "double":
      return `Double the BPM of ${tracks} on ${root} ?`;
    case "suggested":
      return `Apply the suggested half / double fix to ${tracks} on ${root} ?`;
    case "reanalyse":
      return `Replace the beat grid of ${tracks} with a new analysis on ${root} ?`;
  }
}

export function opDescription(kind: BatchKind, lib: LibraryKind = "pacemaker"): string {
  const BOTH = bothNote(lib);
  switch (kind) {
    case "nudge":
      return `Every beat line of each track moves by the same time; the tempo and the bar lines stay. ${BOTH}`;
    case "halve":
      return `Halves the tempo the way the grid editor's ½ BPM does: every second beat line is kept. ${BOTH}`;
    case "double":
      return `Doubles the tempo the way the grid editor's ×2 BPM does: a beat line is added midway between every two. ${BOTH}`;
    case "suggested":
      return `Each track gets the fix its own rule suggested (half or double, whichever brings it into its genre's window). ${BOTH}`;
    case "reanalyse":
      return `Each track's grid is replaced by the analysis shown in the preview, made from the audio already ${lib === "computer" ? "in the library" : "on the device"}. A beat grid or BPM you set by hand is replaced. No audio is copied or changed. ${BOTH}`;
  }
}

/** A short word for why a track is in the "Suspect grid" filter, for the table and inspector. */
export function suspectKinds(s: { tempo_flag: boolean; quality_flag: boolean; fix: "halve" | "double" | null }): string {
  const parts: string[] = [];
  if (s.tempo_flag) parts.push(s.fix === "halve" ? "tempo too high, half fits" : s.fix === "double" ? "tempo too low, double fits" : "tempo outside window");
  if (s.quality_flag) parts.push("low grid confidence");
  return parts.join("; ");
}
