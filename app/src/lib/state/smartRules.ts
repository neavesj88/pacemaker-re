// Pure logic of smart cases, shared by the rule builder and the browser mock:
//  - the menu of rule kinds and a fresh rule of each kind,
//  - validation worded like src-tauri/src/commands/cases/smart/rules.rs (so the builder can say
//    what is wrong before asking the backend),
//  - a plain-language description of a query (shown under the builder),
//  - the browser twin of the Rust evaluation (`evaluateQuery`), used only by devmock/cases.ts.
//    The real app never calls it: the preview, the counts and the device copy all run the Rust
//    code. views/cases/smartRules.test.mjs pins this twin to the numbers the Rust tests use.
// No store or component imports (types only) so it runs under plain node.
import type { Mode, Query, Rule, Sort, SortField, TagMatch, TextField, TextOp } from "../api/cases";
import { LABEL_COLOURS, MAX_ENERGY, MIN_ENERGY, colourName, normaliseTag, tagKey, type TrackLabels } from "./labels.ts";

export const MS_PER_DAY = 86_400_000;
/** Same limits as the backend. */
export const MAX_DEPTH = 8;
export const MAX_NODES = 200;
export const MAX_LIMIT = 100_000;
/** The `colours` entry of a colour rule that stands for "no colour label". */
export const NO_COLOUR = "none";

export const TAG_MATCHES: { how: TagMatch; label: string; words: string }[] = [
  { how: "any", label: "has any of", words: "has any of the tags" },
  { how: "all", label: "has all of", words: "has all of the tags" },
  { how: "none", label: "has none of", words: "has none of the tags" },
];

// ---- menu of kinds ------------------------------------------------------------------------

export const MODES: { mode: Mode; label: string; words: string }[] = [
  { mode: "all", label: "All of these (AND)", words: "all of" },
  { mode: "any", label: "Any of these (OR)", words: "any of" },
  { mode: "none", label: "None of these (NOT)", words: "none of" },
];

export const TEXT_FIELDS: { field: TextField; label: string }[] = [
  { field: "title", label: "Title" },
  { field: "artist", label: "Artist" },
  { field: "album", label: "Album" },
  { field: "genre", label: "Genre" },
  { field: "label", label: "Label" },
  { field: "comments", label: "Comments" },
  { field: "remixer", label: "Remixer" },
];

export const TEXT_OPS: { op: TextOp; label: string }[] = [
  { op: "contains", label: "contains" },
  { op: "is", label: "is" },
  { op: "starts_with", label: "starts with" },
];

export type KindEntry = { id: string; label: string; group: string };

/** The rule-type menu, grouped. `id` is `kindId(rule)`: the rule's kind, or `text:<field>`. */
export const KINDS: KindEntry[] = [
  ...TEXT_FIELDS.map((f) => ({ id: `text:${f.field}`, label: f.label, group: "Text" })),
  { id: "bpm_range", label: "BPM between", group: "Numbers" },
  { id: "key", label: "Key is", group: "Numbers" },
  { id: "key_compatible", label: "Key compatible with", group: "Numbers" },
  { id: "year_range", label: "Year between", group: "Numbers" },
  { id: "duration_range", label: "Length between", group: "Numbers" },
  { id: "rating_min", label: "Rating at least", group: "Numbers" },
  { id: "format", label: "Format is", group: "Numbers" },
  { id: "plays_zero", label: "Never played", group: "Playing" },
  { id: "plays_min", label: "Played at least", group: "Playing" },
  { id: "plays_range", label: "Played between", group: "Playing" },
  { id: "played_within", label: "Last played within", group: "Playing" },
  { id: "last_played_before", label: "Not played for (stale)", group: "Playing" },
  { id: "added_within", label: "Added within the last", group: "Library" },
  { id: "added_before", label: "Added more than … ago", group: "Library" },
  { id: "favourite", label: "Favourites", group: "Library" },
  { id: "in_case", label: "Case membership", group: "Library" },
  { id: "analysed", label: "Analysed", group: "Library" },
  { id: "tags", label: "Tags", group: "Labels" },
  { id: "colour", label: "Colour label", group: "Labels" },
  { id: "energy_range", label: "Energy between", group: "Labels" },
];

export const kindId = (r: Rule): string => (r.kind === "text" ? `text:${r.field}` : r.kind === "group" ? "group" : r.kind);

/** A fresh rule of this kind (`id` as in KINDS), with sensible starting values. */
export function defaultRule(id: string, firstCaseId = NO_CASE): Rule {
  if (id.startsWith("text:")) return { kind: "text", field: id.slice(5) as TextField, op: "contains", text: "" };
  switch (id) {
    case "rating_min": return { kind: "rating_min", stars: 4 };
    case "favourite": return { kind: "favourite", value: true };
    case "plays_min": return { kind: "plays_min", n: 5 };
    case "plays_range": return { kind: "plays_range", min: 1, max: 5 };
    case "played_within": return { kind: "played_within", days: 30 };
    case "last_played_before": return { kind: "last_played_before", days: 90, include_never: true };
    case "bpm_range": return { kind: "bpm_range", min: 120, max: 130 };
    case "key": return { kind: "key", key: "8A" };
    case "key_compatible": return { kind: "key_compatible", key: "8A" };
    case "analysed": return { kind: "analysed", value: true };
    case "year_range": return { kind: "year_range", min: 2000, max: null };
    case "added_within": return { kind: "added_within", days: 30 };
    case "added_before": return { kind: "added_before", days: 365 };
    case "duration_range": return { kind: "duration_range", min_secs: 180, max_secs: 420 };
    case "format": return { kind: "format", value: "MP3" };
    case "in_case": return { kind: "in_case", case_id: firstCaseId, value: true };
    case "tags": return { kind: "tags", how: "any", tags: [] };
    case "colour": return { kind: "colour", colours: ["red"], value: true };
    case "energy_range": return { kind: "energy_range", min: 7, max: null };
    case "group": return newGroup(defaultRule("plays_zero"));
    default: return { kind: "plays_zero" };
  }
}

/** Marks a case rule that has no case yet (refused by validation, shown as "Choose a case"). */
export const NO_CASE = -1;

/** The case a new case rule starts on: the first real case, not the Pacemaker copy of a smart case; NO_CASE when there are none. */
export const defaultCaseId = (infos: { id: number; name: string }[]): number =>
  (infos.find((c) => !c.name.startsWith("★ ")) ?? infos[0])?.id ?? NO_CASE;

type CaseRef = { id: number; name: string };

/**
 * The name a case rule was saved with, when the case that has its id is called something else now.
 * Case ids are only unique within one card, so this is the sign that the rule was written for another
 * card (or that the case was renamed). Null when the names agree, the rule has no saved name, or the
 * case is missing (that has its own message).
 */
export function caseNameDrift(r: Rule, infos: CaseRef[]): { saved: string; now: string } | null {
  if (r.kind !== "in_case" || !r.case_name) return null;
  const now = infos.find((c) => c.id === r.case_id)?.name;
  return now !== undefined && now !== r.case_name ? { saved: r.case_name, now } : null;
}

/** Writes the current case name into every case rule that has none (older rule files), groups included. */
export function stampCaseNames(rules: Rule[], infos: CaseRef[]): void {
  for (const r of rules) {
    if (r.kind === "group") stampCaseNames(r.items, infos);
    else if (r.kind === "in_case" && !r.case_name) {
      const name = infos.find((c) => c.id === r.case_id)?.name;
      if (name !== undefined) r.case_name = name;
    }
  }
}

export const newGroup = (first: Rule, mode: Mode = "any"): Rule => ({ kind: "group", mode, items: [first] });

export const newQuery = (): Query => ({ mode: "all", rules: [defaultRule("rating_min")], sort: null, limit: null });

// ---- sort ---------------------------------------------------------------------------------

type SortKind = "text" | "number" | "date" | "key";
export const SORT_FIELDS: { field: SortField; label: string; kind: SortKind }[] = [
  { field: "title", label: "Title", kind: "text" },
  { field: "artist", label: "Artist", kind: "text" },
  { field: "album", label: "Album", kind: "text" },
  { field: "genre", label: "Genre", kind: "text" },
  { field: "label", label: "Label", kind: "text" },
  { field: "format", label: "Format", kind: "text" },
  { field: "bpm", label: "BPM", kind: "number" },
  { field: "key", label: "Key", kind: "key" },
  { field: "rating", label: "Rating", kind: "number" },
  { field: "year", label: "Year", kind: "number" },
  { field: "duration", label: "Length", kind: "number" },
  { field: "plays", label: "Play count", kind: "number" },
  { field: "file_size", label: "File size", kind: "number" },
  { field: "date_added", label: "Date added", kind: "date" },
  { field: "last_played", label: "Last played", kind: "date" },
];

const sortInfo = (f: SortField) => SORT_FIELDS.find((s) => s.field === f)!;

/** The two direction labels for a field: [ascending, descending]. */
export function directionLabels(f: SortField): [string, string] {
  switch (sortInfo(f).kind) {
    case "text": return ["A to Z", "Z to A"];
    case "date": return ["oldest first", "newest first"];
    case "key": return ["1A to 12B", "12B to 1A"];
    default: return ["low to high", "high to low"];
  }
}

// ---- durations ----------------------------------------------------------------------------

/** "3:30" -> 210, "210" -> 210, "1:02:03" -> 3723, "" -> null; anything else NaN. */
export function parseDuration(text: string): number | null {
  const t = text.trim();
  if (t === "") return null;
  if (!/^\d+(:[0-5]?\d){0,2}$/.test(t)) return Number.NaN;
  return t.split(":").reduce((acc, part) => acc * 60 + Number(part), 0);
}

export function formatDuration(secs: number | null): string {
  if (secs == null || !Number.isFinite(secs)) return "";
  const h = Math.floor(secs / 3600);
  const m = Math.floor((secs % 3600) / 60);
  const s = Math.floor(secs % 60);
  return h > 0 ? `${h}:${String(m).padStart(2, "0")}:${String(s).padStart(2, "0")}` : `${m}:${String(s).padStart(2, "0")}`;
}

// ---- keys (port of Camelot in rules.rs) -----------------------------------------------------

type Cam = { n: number; minor: boolean };
const MINOR = [5, 12, 7, 2, 9, 4, 11, 6, 1, 8, 3, 10];
const MAJOR = [8, 3, 10, 5, 12, 7, 2, 9, 4, 11, 6, 1];

export function camelot(text: string): Cam | null {
  const t = text.trim();
  let m = /^(\d{1,2})([abAB])$/.exec(t);
  if (m) {
    const n = Number(m[1]);
    return n >= 1 && n <= 12 ? { n, minor: m[2].toLowerCase() === "a" } : null;
  }
  m = /^([A-Ga-g])([#b♯♭]?)\s*(maj|major|min|minor|m)?$/i.exec(t);
  if (!m) return null;
  const base = { C: 0, D: 2, E: 4, F: 5, G: 7, A: 9, B: 11 }[m[1].toUpperCase() as "C"];
  const acc = m[2] === "#" || m[2] === "♯" ? 1 : m[2] ? -1 : 0;
  const pc = (((base + acc) % 12) + 12) % 12;
  const word = (m[3] ?? "").toLowerCase();
  const minor = word === "m" || word === "min" || word === "minor";
  return { n: minor ? MINOR[pc] : MAJOR[pc], minor };
}

function compatible(a: Cam, b: Cam): boolean {
  if (a.n === b.n) return true;
  const step = (((a.n - b.n) % 12) + 12) % 12;
  return a.minor === b.minor && (step === 1 || step === 11);
}

// ---- validation ---------------------------------------------------------------------------

const fieldName = (f: TextField) => f;

function checkRange(label: string, min: number | null, max: number | null, lowest: number): string {
  if (Number.isNaN(min) || Number.isNaN(max)) return `The ${label} must be a number.`;
  if (min == null && max == null) return `${label.startsWith("e") ? "An" : "A"} ${label} rule needs a minimum, a maximum or both.`;
  if ((min != null && min < lowest) || (max != null && max < lowest)) return `${label[0].toUpperCase()}${label.slice(1)} values must be ${lowest} or more.`;
  if (min != null && max != null && min > max) return `The ${label} minimum is above the maximum.`;
  return "";
}

const isInt = (n: unknown): n is number => typeof n === "number" && Number.isInteger(n);

function validateRule(r: Rule, depth: number, count: { n: number }): string {
  switch (r.kind) {
    case "rating_min": return r.stars >= 1 && r.stars <= 5 ? "" : "A rating rule needs 1 to 5 stars.";
    case "plays_min": return isInt(r.n) && r.n >= 1 ? "" : "“Played at least” needs a whole number of 1 or more.";
    case "plays_range":
      return !(r.min == null || isInt(r.min)) || !(r.max == null || isInt(r.max)) ? "Play counts must be whole numbers." : checkRange("play count", r.min, r.max, 0);
    case "played_within": return isInt(r.days) && r.days >= 1 ? "" : "“Last played within” needs a whole number of days, 1 or more.";
    case "last_played_before": return isInt(r.days) && r.days >= 1 ? "" : "“Not played for” needs a whole number of days, 1 or more.";
    case "bpm_range": {
      if (r.min == null && r.max == null) return "A BPM rule needs a minimum, a maximum or both.";
      if ((r.min != null && !(Number.isFinite(r.min) && r.min >= 0)) || (r.max != null && !(Number.isFinite(r.max) && r.max >= 0))) return "BPM values must be positive numbers.";
      if (r.min != null && r.max != null && r.min > r.max) return "The BPM minimum is above the maximum.";
      return "";
    }
    case "key":
    case "key_compatible": return camelot(r.key) ? "" : `"${r.key}" is not a key (try 8A or Am).`;
    case "text": return r.text.trim() ? "" : `A ${fieldName(r.field)} rule needs some text.`;
    case "year_range":
      return !(r.min == null || isInt(r.min)) || !(r.max == null || isInt(r.max)) ? "Years must be whole numbers." : checkRange("year", r.min, r.max, 1);
    case "added_within":
    case "added_before": return isInt(r.days) && r.days >= 1 ? "" : "A date-added rule needs a whole number of days, 1 or more.";
    case "duration_range":
      return Number.isNaN(r.min_secs) || Number.isNaN(r.max_secs) ? "Lengths look like 3:30 (or a number of seconds)." : checkRange("duration", r.min_secs, r.max_secs, 0);
    case "format": return r.value.trim() ? "" : "A format rule needs a format such as MP3.";
    case "in_case": return Number.isInteger(r.case_id) && r.case_id >= 0 ? "" : "A case rule needs a case.";
    case "tags": {
      if (r.tags.length === 0) return "A tag rule needs at least one tag.";
      for (const t of r.tags) {
        const n = normaliseTag(t);
        if (!n.ok) return n.error;
      }
      return "";
    }
    case "colour": {
      if (r.colours.length === 0) return "A colour rule needs at least one colour.";
      const bad = r.colours.find((c) => c !== NO_COLOUR && !(LABEL_COLOURS as readonly string[]).includes(c));
      return bad === undefined ? "" : `"${bad}" is not one of the colour labels.`;
    }
    case "energy_range": {
      if (!(r.min == null || isInt(r.min)) || !(r.max == null || isInt(r.max))) return "Energy values must be whole numbers.";
      const e = checkRange("energy", r.min, r.max, MIN_ENERGY);
      if (e) return e;
      return (r.min != null && r.min > MAX_ENERGY) || (r.max != null && r.max > MAX_ENERGY) ? `Energy values must be ${MAX_ENERGY} or less.` : "";
    }
    case "group":
      if (r.items.length === 0) return "A group needs at least one rule.";
      return validateItems(r.items, depth + 1, count);
    default: return "";
  }
}

function validateItems(items: Rule[], depth: number, count: { n: number }): string {
  if (depth > MAX_DEPTH) return `Groups can be nested at most ${MAX_DEPTH} deep.`;
  for (const r of items) {
    if (++count.n > MAX_NODES) return `A smart case can have at most ${MAX_NODES} rules and groups.`;
    const e = validateRule(r, depth, count);
    if (e) return e;
  }
  return "";
}

/** Why the query cannot be saved or run, worded for the user; "" when it is fine. */
export function validateQuery(q: Query): string {
  if (q.rules.length === 0 && q.limit == null) return "Add at least one rule, or set a limit to take the first tracks of the whole library.";
  if (q.limit != null && !(isInt(q.limit) && q.limit >= 1 && q.limit <= MAX_LIMIT)) return `The limit must be a whole number between 1 and ${MAX_LIMIT}.`;
  return validateItems(q.rules, 1, { n: 0 });
}

// ---- describing ---------------------------------------------------------------------------

const range = (min: number | null, max: number | null, unit = "", fmt: (n: number) => string = String) =>
  min != null && max != null ? `between ${fmt(min)} and ${fmt(max)}${unit}` : min != null ? `at least ${fmt(min)}${unit}` : max != null ? `at most ${fmt(max!)}${unit}` : "any";

const plural = (n: number, one: string, many = `${one}s`) => `${n} ${n === 1 ? one : many}`;
const COLOUR_WORD: Record<string, true> = Object.fromEntries(LABEL_COLOURS.map((c) => [c, true]));

/** One rule in words. `caseName` turns a case id into its name (or says it is missing). */
export function describeRule(r: Rule, caseName: (id: number) => string): string {
  switch (r.kind) {
    case "rating_min": return `rated ${r.stars}+ stars`;
    case "favourite": return r.value ? "in Favourites" : "not in Favourites";
    case "plays_zero": return "never played";
    case "plays_min": return `played at least ${plural(r.n, "time")}`;
    case "plays_range": return `played ${range(r.min, r.max, " times")}`;
    case "played_within": return `played in the last ${plural(r.days, "day")}`;
    case "last_played_before": return `not played for ${plural(r.days, "day")} (${r.include_never ? "tracks never played count" : "tracks never played do not count"})`;
    case "bpm_range": return `BPM ${range(r.min, r.max)}`;
    case "key": return `key is ${r.key.trim()}`;
    case "key_compatible": return `key mixes with ${r.key.trim()}`;
    case "analysed": return r.value ? "analysed" : "not analysed";
    case "text": return `${r.field} ${TEXT_OPS.find((o) => o.op === r.op)!.label} “${r.text.trim()}”`;
    case "year_range": return `year ${range(r.min, r.max)}`;
    case "added_within": return `added in the last ${plural(r.days, "day")}`;
    case "added_before": return `added more than ${plural(r.days, "day")} ago`;
    case "duration_range": return `length ${range(r.min_secs, r.max_secs, "", formatDuration)}`;
    case "format": return `format is ${r.value.trim()}`;
    case "in_case": return `${r.value ? "in" : "not in"} case “${caseName(r.case_id)}”`;
    case "tags": return `${TAG_MATCHES.find((m) => m.how === r.how)!.words} ${r.tags.map((t) => `“${t}”`).join(", ")}`;
    case "colour": return `colour label ${r.value ? "is" : "is not"} ${r.colours.map((c) => (c === NO_COLOUR ? "no colour" : c in COLOUR_WORD ? colourName(c as (typeof LABEL_COLOURS)[number]).toLowerCase() : c)).join(" or ")}`;
    case "energy_range": return `energy ${range(r.min, r.max)}`;
    case "group": return `${MODES.find((m) => m.mode === r.mode)!.words} (${r.items.map((i) => describeRule(i, caseName)).join("; ")})`;
  }
}

export function describeSort(s: Sort | null): string {
  if (!s) return "";
  const [asc, desc] = directionLabels(s.field);
  return `Ordered by ${sortInfo(s.field).label.toLowerCase()}, ${s.descending ? desc : asc}.`;
}

/** The whole query as a sentence, e.g. "Tracks that match all of: rated 4+ stars; never played. Ordered by BPM, high to low. Only the first 50." */
export function describeQuery(q: Query, caseName: (id: number) => string): string {
  const head =
    q.rules.length === 0
      ? "Every track."
      : `Tracks that match ${MODES.find((m) => m.mode === q.mode)!.words}: ${q.rules.map((r) => describeRule(r, caseName)).join("; ")}.`;
  const order = describeSort(q.sort);
  const limit = q.limit != null ? `Only the first ${plural(q.limit, "track")}.` : "";
  return [head, order, limit].filter(Boolean).join(" ");
}

// ---- evaluation (browser twin of rules.rs; the real app never calls it) --------------------------

/** A track with the columns the rules read (the Track payload plus the extras the backend loads). */
export type RuleRow = {
  track_id: number;
  title: string;
  artist: string;
  album: string;
  genre: string;
  bpm: number;
  key: string;
  play_time_secs: number;
  rating: number;
  file_size: number;
  format: string;
  analysed: boolean;
  times_played: number;
  /** Unix ms, -1 or 0 = never. */
  last_played: number;
  year: number;
  /** Unix ms, 0 = unknown. */
  date_added: number;
  label: string;
  comments: string;
  remixer: string;
};

export type EvalEnv = {
  favourites: Set<number>;
  /** Tags and colour of a track (app-side labels); undefined = none. */
  labelOf: (id: number) => Pick<TrackLabels, "tags" | "colour"> | undefined;
  /** Whole-level energy under the Energy source setting (state/energyResolve.ts), or null. */
  energyOf: (id: number) => number | null;
  /** Members of each case a rule names; null for a case that does not exist. */
  cases: Map<number, Set<number> | null>;
  now: number;
};

const inRange = (v: number, min: number | null, max: number | null) => (min == null || v >= min) && (max == null || v <= max);

function textOf(f: TextField, t: RuleRow): string {
  return t[f];
}

export function ruleMatches(r: Rule, t: RuleRow, env: EvalEnv): boolean {
  const ago = (days: number) => env.now - days * MS_PER_DAY;
  switch (r.kind) {
    case "rating_min": return t.rating >= r.stars;
    case "favourite": return env.favourites.has(t.track_id) === r.value;
    case "plays_zero": return t.times_played === 0;
    case "plays_min": return t.times_played >= r.n;
    case "plays_range": return inRange(t.times_played, r.min, r.max);
    case "played_within": return t.last_played > 0 && t.last_played >= ago(r.days);
    case "last_played_before": return t.last_played > 0 ? t.last_played < ago(r.days) : r.include_never;
    case "bpm_range": return t.bpm > 0 && inRange(t.bpm, r.min, r.max);
    case "key": { const a = camelot(r.key), b = camelot(t.key); return !!a && !!b && a.n === b.n && a.minor === b.minor; }
    case "key_compatible": { const a = camelot(r.key), b = camelot(t.key); return !!a && !!b && compatible(a, b); }
    case "analysed": return t.analysed === r.value;
    case "text": {
      const have = textOf(r.field, t).trim().toLowerCase();
      const want = r.text.trim().toLowerCase();
      return r.op === "contains" ? have.includes(want) : r.op === "is" ? have === want : have.startsWith(want);
    }
    case "year_range": return t.year > 0 && inRange(t.year, r.min, r.max);
    case "added_within": return t.date_added > 0 && t.date_added >= ago(r.days);
    case "added_before": return t.date_added > 0 && t.date_added < ago(r.days);
    case "duration_range": return t.play_time_secs > 0 && inRange(t.play_time_secs, r.min_secs, r.max_secs);
    case "format": return t.format.trim().toLowerCase() === r.value.trim().toLowerCase();
    case "in_case": return (env.cases.get(r.case_id)?.has(t.track_id) ?? false) === r.value;
    case "tags": {
      const have = new Set((env.labelOf(t.track_id)?.tags ?? []).map(tagKey));
      const want = r.tags.map((g) => { const n = normaliseTag(g); return tagKey(n.ok ? n.tag : g); });
      return r.how === "any" ? want.some((g) => have.has(g)) : r.how === "all" ? want.every((g) => have.has(g)) : !want.some((g) => have.has(g));
    }
    case "colour": return r.colours.includes(env.labelOf(t.track_id)?.colour ?? NO_COLOUR) === r.value;
    case "energy_range": {
      const e = env.energyOf(t.track_id);
      return e !== null && inRange(e, r.min, r.max);
    }
    case "group": return groupMatches(r.mode, r.items, t, env);
  }
}

function groupMatches(mode: Mode, items: Rule[], t: RuleRow, env: EvalEnv): boolean {
  if (mode === "all") return items.every((r) => ruleMatches(r, t, env));
  if (mode === "any") return items.some((r) => ruleMatches(r, t, env));
  return !items.some((r) => ruleMatches(r, t, env));
}

export function referencedCases(rules: Rule[], out = new Set<number>()): Set<number> {
  for (const r of rules) {
    if (r.kind === "in_case") out.add(r.case_id);
    else if (r.kind === "group") referencedCases(r.items, out);
  }
  return out;
}

type SortVal = null | number | string;

function sortVal(f: SortField, t: RuleRow): SortVal {
  const text = (s: string) => (s.trim() === "" ? null : s.trim().toLowerCase());
  const num = (v: number, known: boolean) => (known ? v : null);
  switch (f) {
    case "title": return text(t.title);
    case "artist": return text(t.artist);
    case "album": return text(t.album);
    case "genre": return text(t.genre);
    case "label": return text(t.label);
    case "format": return text(t.format);
    case "bpm": return num(t.bpm, t.bpm > 0);
    case "key": { const c = camelot(t.key); return c ? c.n * 2 + (c.minor ? 0 : 1) : null; }
    case "rating": return t.rating;
    case "year": return num(t.year, t.year > 0);
    case "duration": return num(t.play_time_secs, t.play_time_secs > 0);
    case "date_added": return num(t.date_added, t.date_added > 0);
    case "plays": return t.times_played;
    case "last_played": return num(t.last_played, t.last_played > 0);
    case "file_size": return num(t.file_size, t.file_size > 0);
  }
}

const cmp = (a: SortVal, b: SortVal, descending: boolean): number => {
  if (a === null && b === null) return 0;
  if (a === null) return 1;
  if (b === null) return -1;
  const o = a < b ? -1 : a > b ? 1 : 0;
  return descending ? -o : o;
};

const tie = (a: RuleRow, b: RuleRow) => {
  const x = a.artist.toLowerCase(), y = b.artist.toLowerCase();
  if (x !== y) return x < y ? -1 : 1;
  const p = a.title.toLowerCase(), q = b.title.toLowerCase();
  if (p !== q) return p < q ? -1 : 1;
  return a.track_id - b.track_id;
};

const DASH = "–";

/** "2023-11-14" for Unix ms (UTC), an en dash for none. */
export const dateLabel = (ms: number) => (ms > 0 ? new Date(ms).toISOString().slice(0, 10) : DASH);

/** The preview's "value the order is based on" for fields the track table has no column for; null for the others. */
function valueLabel(f: SortField, t: RuleRow): string | null {
  switch (f) {
    case "label": return t.label.trim() || DASH;
    case "format": return t.format.trim() || DASH;
    case "year": return t.year > 0 ? String(t.year) : DASH;
    case "date_added": return dateLabel(t.date_added);
    case "last_played": return t.last_played > 0 ? dateLabel(t.last_played) : "never";
    case "rating": return t.rating > 0 ? "★".repeat(t.rating) : DASH;
    case "plays": return String(t.times_played);
    case "file_size": return `${(t.file_size / 1_048_576).toFixed(1)} MB`;
    default: return null;
  }
}

/** What `rules.rs::evaluate` returns. */
export function evaluateQuery(q: Query, rows: RuleRow[], env: EvalEnv): { ids: number[]; matched: number; values: string[] } {
  for (const id of [...referencedCases(q.rules)].sort((a, b) => a - b)) {
    if (!env.cases.get(id)) throw `A rule refers to a case that no longer exists (case ${id}). Pick another case.`;
  }
  const hits = rows.filter((t) => q.rules.length === 0 || groupMatches(q.mode, q.rules, t, env));
  const matched = hits.length;
  if (q.sort) {
    const { field, descending } = q.sort;
    hits.sort((a, b) => cmp(sortVal(field, a), sortVal(field, b), descending) || tie(a, b));
  } else hits.sort(tie);
  const cut = q.limit != null ? hits.slice(0, q.limit) : hits;
  const first = q.sort && cut.length > 0 ? valueLabel(q.sort.field, cut[0]) : null;
  const values = q.sort && first !== null ? cut.map((t) => valueLabel(q.sort!.field, t)!) : [];
  return { ids: cut.map((t) => t.track_id), matched, values };
}
