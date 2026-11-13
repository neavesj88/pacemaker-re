// The typed query language of the search box, as a pure module (no store, no component, only
// types from the wire layer) so it runs under plain node:
// `node src/lib/views/tracks/query.test.mjs`.
//
//   words                  every word must be found in the title, artist, album, genre, key,
//                          comments, label, remixer, composer, year or a tag
//   "two words"            a phrase, found as written
//   field:value            title artist album genre label remixer composer comments format (contains)
//   year:2010-2015         ranges (2010..2015 works too, both ends included)
//   bpm>=128 plays>10      comparisons (>, >=, <, <=, =), also written bpm:>=128
//   key:8A  key:~8A        the key / keys that mix with it (Camelot or musical name, key:Am)
//   tag:peak  colour:red   app-side labels
//   energy:7-9             energy under the "Energy source" setting (your rating, the audio estimate,
//                          or yours else the estimate); myenergy: and autoenergy: ask for one source
//   -term                  not
//   a OR b                 either side; AND binds tighter, so  x y OR z  is  (x y) OR z
//   is:fav                 flags
//
// Nothing here throws: a term it cannot read is skipped and reported in `diagnostics`, so a
// half-typed query never blanks the list or breaks the table.
import type { Track } from "$lib/api/types";
import { LABEL_COLOURS } from "../../state/labels.ts";
import { keysCompatible, parseKey } from "./trackLogic.ts";

export type QueryLabels = { tags: readonly string[]; colour: string | null; energy: number | null };

/** What a query needs to know about a track beyond the track itself. */
export type QueryContext = {
  labelOf: (id: number) => QueryLabels | undefined;
  /** Whole-level energy (1 to 10) under the "Energy source" setting, or null (state/energyResolve.ts). */
  energyOf: (id: number) => number | null;
  /** Your own energy rating, or null. */
  myEnergyOf: (id: number) => number | null;
  /** The estimate from the audio (1.0 to 10.0, unrounded), or null. */
  autoEnergyOf: (id: number) => number | null;
  /** Tracks that need analysis (their BPM is not shown, so it is not matched either). */
  needs: { has(id: number): boolean };
  isFavourite: (id: number) => boolean;
  /** Unix milliseconds; relative dates (`added:7d`) count back from it. */
  now: number;
};

export type QueryDiagnostic = { term: string; message: string };

export type CompiledQuery = {
  /** No usable term: everything matches. */
  empty: boolean;
  diagnostics: QueryDiagnostic[];
  test: (t: Track, cx: QueryContext) => boolean;
};

// ---- tokens -------------------------------------------------------------------------------

export type Token = {
  /** The text as typed, for messages. */
  raw: string;
  negated: boolean;
  /** Lower-cased field name when the token has `field<op>value`, else null. */
  field: string | null;
  op: Op;
  value: string;
  /** The value (or the whole token) was in quotes: it is a phrase, never a range or comparison. */
  quoted: boolean;
};

export type Op = ":" | "=" | ">" | ">=" | "<" | "<=";

const OP_CHARS = new Set([":", "=", ">", "<"]);

/** Splits the text into tokens and `OR` separators (null). Lenient: an unclosed quote runs to the end. */
export function tokenize(text: string): (Token | null)[] {
  const out: (Token | null)[] = [];
  const n = text.length;
  let i = 0;
  while (i < n) {
    if (/\s/.test(text[i])) {
      i++;
      continue;
    }
    const start = i;
    let negated = false;
    if (text[i] === "-" && i + 1 < n && !/\s/.test(text[i + 1])) {
      negated = true;
      i++;
    }
    let value = "";
    let quoted = false;
    let sawQuote = false;
    let field: string | null = null;
    let op: Op = ":";
    if (text[i] === '"') {
      // a bare phrase
      const end = text.indexOf('"', i + 1);
      value = text.slice(i + 1, end < 0 ? n : end);
      i = end < 0 ? n : end + 1;
      quoted = true;
      sawQuote = true;
    } else {
      // field name, then operator, then value (quoted or not)
      let j = i;
      while (j < n && !/\s/.test(text[j]) && text[j] !== '"' && !OP_CHARS.has(text[j])) j++;
      const name = text.slice(i, j);
      let k = j;
      while (k < n && OP_CHARS.has(text[k])) k++;
      const opText = text.slice(j, k);
      const parsed = parseOp(opText);
      if (name && /^[A-Za-z_]+$/.test(name) && parsed && k > j) {
        field = name.toLowerCase();
        op = parsed.op;
        i = k;
        // a leftover operator character after the real one (`bpm>>3`) stays in the value
        if (parsed.rest) value = parsed.rest;
      }
      if (text[i] === '"') {
        const end = text.indexOf('"', i + 1);
        value += text.slice(i + 1, end < 0 ? n : end);
        i = end < 0 ? n : end + 1;
        quoted = true;
        sawQuote = true;
      }
      // the rest of the run (until whitespace)
      let m = i;
      while (m < n && !/\s/.test(text[m])) m++;
      if (m > i) {
        const rest = text.slice(i, m);
        value += rest.replace(/"/g, "");
        if (rest.includes('"')) sawQuote = true;
        i = m;
      }
    }
    const raw = text.slice(start, i);
    if (!field && !sawQuote && !negated && value === "OR") {
      out.push(null);
      continue;
    }
    out.push({ raw, negated, field, op, value, quoted });
  }
  return out;
}

/** `>=` and friends: the operator at the start of `text` and anything left over. */
function parseOp(text: string): { op: Op; rest: string } | null {
  for (const op of [":>=", ":<=", ":>", ":<", ":=", ">=", "<=", ">", "<", "=", ":"] as const) {
    if (text.startsWith(op)) return { op: op.replace(/^:(?=.)/, "") as Op, rest: text.slice(op.length) };
  }
  return null;
}

// ---- fields -------------------------------------------------------------------------------

type Kind = "text" | "number" | "date" | "key" | "tag" | "colour" | "flag";

type FieldDef = {
  name: string;
  aliases: string[];
  kind: Kind;
  /** For text: the track strings searched. For number/date: the getter. */
  text?: (t: Track) => string;
  num?: (t: Track, cx: QueryContext) => number | null;
  /** Number fields: what a bare number means. */
  tolerance?: number;
  /** Number fields: a value with units (size, time) goes through this instead of Number(). */
  parse?: (s: string) => number | null;
  help: string;
};

const num = (n: number): number | null => (Number.isFinite(n) ? n : null);

/** `3:30` or `210` (seconds) or `3:30:00`; null when it is neither. */
export function parseDuration(s: string): number | null {
  const m = /^(\d+)(?::([0-5]?\d))?(?::([0-5]?\d))?(?:\.\d+)?$/.exec(s.trim());
  if (!m) return null;
  const a = Number(m[1]);
  if (m[3] !== undefined) return a * 3600 + Number(m[2]) * 60 + Number(m[3]);
  if (m[2] !== undefined) return a * 60 + Number(m[2]);
  return a;
}

const SIZE_UNITS: Record<string, number> = { k: 1024, kb: 1024, m: 1024 ** 2, mb: 1024 ** 2, g: 1024 ** 3, gb: 1024 ** 3 };

/** `10` (megabytes), `500k`, `10m`, `1.5g`, `700mb`, in bytes; null when unreadable. */
export function parseSize(s: string): number | null {
  const m = /^(\d+(?:\.\d+)?)\s*(k|kb|m|mb|g|gb)?$/i.exec(s.trim());
  if (!m) return null;
  return Number(m[1]) * (m[2] ? SIZE_UNITS[m[2].toLowerCase()] : 1024 ** 2);
}

const plainNumber = (s: string): number | null => (/^\d+(?:\.\d+)?$/.test(s.trim()) ? num(Number(s)) : null);

const textField = (name: string, aliases: string[], get: (t: Track) => string, help: string): FieldDef => ({ name, aliases, kind: "text", text: get, help });

export const FIELDS: FieldDef[] = [
  textField("title", ["t"], (t) => t.title, "title contains"),
  textField("artist", ["a"], (t) => t.artist, "artist contains"),
  textField("album", [], (t) => t.album, "album contains"),
  textField("genre", [], (t) => t.genre, "genre contains"),
  textField("label", [], (t) => t.label, "record label contains"),
  textField("remixer", ["remix"], (t) => t.remixer, "remixer contains"),
  textField("composer", [], (t) => t.composer, "composer contains"),
  textField("comments", ["comment", "note"], (t) => t.comments, "comments contain"),
  textField("format", ["fmt", "type"], (t) => t.format, "file format contains (mp3, wav, ...)"),
  { name: "year", aliases: [], kind: "number", num: (t) => (t.year > 0 ? t.year : null), help: "release year" },
  { name: "bpm", aliases: ["tempo"], kind: "number", tolerance: 0.5, num: (t, cx) => (t.bpm > 0 && !cx.needs.has(t.track_id) ? t.bpm : null), help: "tempo (bpm:128 means 127.5 to 128.5)" },
  { name: "rating", aliases: ["stars", "rate"], kind: "number", num: (t) => t.rating, help: "stars, 0 to 5" },
  { name: "energy", aliases: [], kind: "number", num: (t, cx) => cx.energyOf(t.track_id), help: "energy 1 to 10 under the Energy source setting (your rating, the audio estimate, or yours else the estimate)" },
  { name: "myenergy", aliases: [], kind: "number", num: (t, cx) => cx.myEnergyOf(t.track_id), help: "your own energy rating, 1 to 10, whatever the setting" },
  { name: "autoenergy", aliases: [], kind: "number", tolerance: 0.5, num: (t, cx) => cx.autoEnergyOf(t.track_id), help: "the energy estimated from the audio, 1 to 10 (autoenergy:7 means 6.5 to 7.5)" },
  { name: "plays", aliases: ["playcount", "count"], kind: "number", num: (t) => t.times_played, help: "times played" },
  { name: "bitrate", aliases: ["br"], kind: "number", num: (t) => (t.bit_rate > 0 ? t.bit_rate : null), help: "bit rate in kbps" },
  { name: "size", aliases: [], kind: "number", parse: parseSize, num: (t) => (t.file_size > 0 ? t.file_size : null), help: "file size; a bare number is MB, or 500k, 10m, 1g" },
  { name: "time", aliases: ["duration", "length", "len"], kind: "number", parse: parseDuration, num: (t) => (t.play_time_secs > 0 ? t.play_time_secs : null), help: "duration, seconds or m:ss" },
  { name: "added", aliases: ["date_added", "dateadded"], kind: "date", num: (t) => (t.date_added > 0 ? t.date_added : null), help: "date added" },
  { name: "played", aliases: ["lastplayed", "last_played"], kind: "date", num: (t) => (t.last_played > 0 ? t.last_played : null), help: "last played (played:never for unplayed)" },
  { name: "key", aliases: [], kind: "key", help: "musical key" },
  { name: "tag", aliases: ["tags"], kind: "tag", help: "has this tag (app-side)" },
  { name: "colour", aliases: ["color"], kind: "colour", help: "colour label (app-side)" },
  { name: "is", aliases: [], kind: "flag", help: "a flag" },
];

const FIELD_BY_NAME = new Map<string, FieldDef>(FIELDS.flatMap((f) => [f.name, ...f.aliases].map((n) => [n, f] as const)));

export const FLAGS = ["fav", "needs", "analysed", "unrated", "unplayed", "untagged"] as const;
type Flag = (typeof FLAGS)[number];
const FLAG_ALIASES: Record<string, Flag> = { fav: "fav", favourite: "fav", favorite: "fav", needs: "needs", unanalysed: "needs", analysed: "analysed", analyzed: "analysed", unrated: "unrated", unplayed: "unplayed", never: "unplayed", untagged: "untagged" };

// ---- terms --------------------------------------------------------------------------------

type Test = (t: Track, cx: QueryContext) => boolean;

type Term = { negated: boolean; test: Test };

type Cmp = (v: number) => boolean;

const DAY = 86_400_000;
const REL_UNITS: Record<string, number> = { d: DAY, w: 7 * DAY, m: 30 * DAY, y: 365 * DAY };

/** The comparison a number term makes, or an error message. `parse` reads one bound. */
function numberCmp(op: Op, value: string, parse: (s: string) => number | null, tolerance: number, quoted: boolean): Cmp | string {
  const v = value.trim();
  const bad = "expected a number, a range like 120-128, or a comparison like >=120";
  if (op === ":" || op === "=") {
    if (!quoted) {
      const range = /^(.+?)(?:\.\.|(?<=[\d\w])-)(.+)$/.exec(v);
      if (range) {
        const lo = parse(range[1]);
        const hi = parse(range[2]);
        if (lo === null || hi === null) return bad;
        const [a, b] = lo <= hi ? [lo, hi] : [hi, lo];
        return (x) => x >= a && x <= b;
      }
    }
    const n = parse(v);
    if (n === null) return bad;
    return tolerance > 0 ? (x) => x >= n - tolerance && x <= n + tolerance : (x) => x === n;
  }
  const n = parse(v);
  if (n === null) return "expected a number after " + op;
  switch (op) {
    case ">": return (x) => x > n;
    case ">=": return (x) => x >= n;
    case "<": return (x) => x < n;
    default: return (x) => x <= n;
  }
}

/** Start (inclusive) and end (exclusive) of a `2026`, `2026-03` or `2026-03-14` date, local time. */
export function dateSpan(s: string): { start: number; end: number } | null {
  const m = /^(\d{4})(?:-(\d{1,2})(?:-(\d{1,2}))?)?$/.exec(s.trim());
  if (!m) return null;
  const y = Number(m[1]);
  const mo = m[2] === undefined ? 1 : Number(m[2]);
  const d = m[3] === undefined ? 1 : Number(m[3]);
  const start = new Date(y, mo - 1, d);
  // reject 2026-13 and 2026-02-31 (Date would roll them over)
  if (start.getFullYear() !== y || start.getMonth() !== mo - 1 || start.getDate() !== d) return null;
  const end = m[3] !== undefined ? new Date(y, mo - 1, d + 1) : m[2] !== undefined ? new Date(y, mo, 1) : new Date(y + 1, 0, 1);
  return { start: start.getTime(), end: end.getTime() };
}

/** The test a date term makes on a time in milliseconds. */
function dateCmp(op: Op, value: string, now: () => number): ((ms: number) => boolean) | string {
  const v = value.trim();
  const bad = "expected a date like 2026-03-14, 2026-03 or 2026, a span like 2026-01-01..2026-03-01, or an age like 7d, 2w, 3m, 1y";
  const rel = /^(\d+)([dwmy])$/i.exec(v);
  if (rel) {
    const age = Number(rel[1]) * REL_UNITS[rel[2].toLowerCase()];
    // an age: `:7d` within the last 7 days, `<7d` newer than that, `>7d` older than that
    switch (op) {
      case ":":
      case "=":
      case "<=": return (t) => t >= now() - age;
      case "<": return (t) => t > now() - age;
      case ">": return (t) => t < now() - age;
      default: return (t) => t <= now() - age; // >=
    }
  }
  const span = /^(.+?)\.\.(.+)$/.exec(v);
  if (span && (op === ":" || op === "=")) {
    const a = dateSpan(span[1]);
    const b = dateSpan(span[2]);
    if (!a || !b) return bad;
    return (t) => t >= a.start && t < b.end;
  }
  const d = dateSpan(v);
  if (!d) return bad;
  switch (op) {
    case ":":
    case "=": return (t) => t >= d.start && t < d.end;
    case ">": return (t) => t >= d.end;
    case ">=": return (t) => t >= d.start;
    case "<": return (t) => t < d.start;
    default: return (t) => t < d.end; // <=
  }
}

/** `*` in a tag pattern matches any run of characters; everything else is literal. */
function tagMatcher(pattern: string): (tag: string) => boolean {
  const p = pattern.trim().toLowerCase();
  if (!p.includes("*")) return (tag) => tag.toLowerCase() === p;
  const re = new RegExp("^" + p.split("*").map((s) => s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")).join(".*") + "$");
  return (tag) => re.test(tag.toLowerCase());
}

const COLOUR_NAMES: readonly string[] = LABEL_COLOURS;

function haystack(t: Track): string[] {
  return [t.title, t.artist, t.album, t.genre, t.key, t.comments, t.label, t.remixer, t.composer, t.year > 0 ? String(t.year) : ""];
}

/** Builds the term for one token, or returns an error message (the term is then skipped) or null
 *  (nothing typed yet after the colon: skipped silently). */
function buildTerm(tok: Token): Test | string | null {
  const field = tok.field ? FIELD_BY_NAME.get(tok.field) : undefined;
  if (!field) {
    // plain text: a word, a phrase, or something with a colon that is not a field
    const needle = tok.raw.replace(/^-/, "");
    const text = (tok.field ? needle.replace(/"/g, "") : tok.value).toLowerCase();
    // nothing yet, or a lone minus that is about to become a negation
    if (text === "" || (text === "-" && !tok.quoted)) return null;
    return (t, cx) => haystack(t).some((h) => h.toLowerCase().includes(text)) || (cx.labelOf(t.track_id)?.tags.some((g) => g.toLowerCase().includes(text)) ?? false);
  }
  const value = tok.value.trim();
  const emptyQuoted = tok.quoted && value === "";
  if (value === "" && !emptyQuoted) return null; // `bpm:` while typing
  switch (field.kind) {
    case "text": {
      const get = field.text!;
      if (emptyQuoted) return (t) => get(t).trim() === "";
      if (tok.op !== ":" && tok.op !== "=") return `${field.name} takes ${field.name}:text`;
      const needle = value.toLowerCase();
      return (t) => get(t).toLowerCase().includes(needle);
    }
    case "number": {
      const parse = field.parse ?? plainNumber;
      const cmp = numberCmp(tok.op, value, parse, field.tolerance ?? 0, tok.quoted);
      if (typeof cmp === "string") return `${field.name}: ${cmp}`;
      const get = field.num!;
      return (t, cx) => {
        const v = get(t, cx);
        return v !== null && cmp(v);
      };
    }
    case "date": {
      const get = field.num!;
      if (/^(never|none)$/i.test(value)) return (t, cx) => get(t, cx) === null;
      let nowNow = 0;
      const cmp = dateCmp(tok.op, value, () => nowNow);
      if (typeof cmp === "string") return `${field.name}: ${cmp}`;
      return (t, cx) => {
        const v = get(t, cx);
        if (v === null) return false;
        nowNow = cx.now;
        return cmp(v);
      };
    }
    case "key": {
      if (/^none$/i.test(value)) return (t) => parseKey(t.key) === null;
      const compat = value.startsWith("~");
      const spec = compat ? value.slice(1).trim() : value;
      if (spec === "") return null;
      if (!compat && /^(1[0-2]|[1-9])$/.test(spec)) {
        const n = Number(spec);
        return (t) => parseKey(t.key)?.num === n;
      }
      const wanted = parseKey(spec);
      if (!wanted) return `key: "${spec}" is not a key (try 8A, Am, F#m)`;
      return compat ? (t) => {
        const k = parseKey(t.key);
        return k !== null && keysCompatible(wanted, k);
      } : (t) => parseKey(t.key)?.code === wanted.code;
    }
    case "tag": {
      const match = tagMatcher(value);
      return (t, cx) => cx.labelOf(t.track_id)?.tags.some(match) ?? false;
    }
    case "colour": {
      const name = value.toLowerCase();
      if (name === "none") return (t, cx) => !cx.labelOf(t.track_id)?.colour;
      if (!COLOUR_NAMES.includes(name)) return `colour: "${value}" is not one of ${COLOUR_NAMES.join(", ")}, none`;
      return (t, cx) => cx.labelOf(t.track_id)?.colour === name;
    }
    default: {
      const flag = FLAG_ALIASES[value.toLowerCase()];
      if (!flag) return `is: "${value}" is not one of ${FLAGS.join(", ")}`;
      switch (flag) {
        case "fav": return (t, cx) => cx.isFavourite(t.track_id);
        case "needs": return (t, cx) => cx.needs.has(t.track_id);
        case "analysed": return (t, cx) => !cx.needs.has(t.track_id);
        case "unrated": return (t) => t.rating === 0;
        case "unplayed": return (t) => t.times_played === 0 && !(t.last_played > 0);
        default: return (t, cx) => !(cx.labelOf(t.track_id)?.tags.length ?? 0);
      }
    }
  }
}

let lastText: string | null = null;
let lastCompiled: CompiledQuery | null = null;

/** Reads the text of the search box. The same text is read once: the table's search and the
 *  filter bar's diagnostics ask for it on every change, and a compiled query holds no state. */
export function compileQuery(text: string): CompiledQuery {
  if (lastCompiled !== null && text === lastText) return lastCompiled;
  const compiled = readQuery(text);
  lastText = text;
  lastCompiled = compiled;
  return compiled;
}

function readQuery(text: string): CompiledQuery {
  const diagnostics: QueryDiagnostic[] = [];
  const groups: Term[][] = [[]];
  for (const tok of tokenize(text)) {
    if (tok === null) {
      if (groups[groups.length - 1].length > 0) groups.push([]);
      continue;
    }
    // `name:value` with a name that is not a field: search for the text, and say so
    if (tok.field && !FIELD_BY_NAME.has(tok.field) && tok.op === ":" && tok.value !== "" && !tok.quoted) {
      diagnostics.push({ term: tok.raw, message: `"${tok.field}" is not a field, so ${tok.raw.replace(/^-/, "")} is searched as text. Put it in quotes to search for it on purpose.` });
    }
    const built = buildTerm(tok);
    if (built === null) continue;
    if (typeof built === "string") {
      diagnostics.push({ term: tok.raw, message: built });
      continue;
    }
    groups[groups.length - 1].push({ negated: tok.negated, test: built });
  }
  const used = groups.filter((g) => g.length > 0);
  if (used.length === 0) return { empty: true, diagnostics, test: () => true };
  return {
    empty: false,
    diagnostics,
    test: (t, cx) => used.some((g) => g.every((term) => term.test(t, cx) !== term.negated)),
  };
}

// ---- help text ----------------------------------------------------------------------------

export type HelpRow = { syntax: string; meaning: string; example: string };

/** The rows of the help popover. Every `example` is compiled by the tests, so the help cannot
 *  describe syntax that does not work. */
export const QUERY_HELP: HelpRow[] = [
  { syntax: "words", meaning: "Every word must appear in the title, artist, album, genre, key, comments, label, remixer, composer, year or a tag", example: "deep house" },
  { syntax: '"a phrase"', meaning: "Exactly as written, spaces included", example: '"summer night"' },
  { syntax: "field:text", meaning: "title, artist, album, genre, label, remixer, composer, comments, format contain the text; field:\"\" finds tracks where it is empty", example: "label:example remixer:\"example remixer\"" },
  { syntax: "bpm:120-128", meaning: "A range, both ends included (120..128 works too). Also year, rating, energy, plays, bitrate, size, time", example: "bpm:120-128 year:2010-2015" },
  { syntax: "rating>=4", meaning: "Comparisons: > >= < <= =. bpm:128 alone means 127.5 to 128.5", example: "rating>=4 energy>=7 plays>10" },
  { syntax: "time>3:30", meaning: "Duration in seconds or m:ss; size in MB (500k, 10m, 1g also work); bitrate in kbps", example: "time:3:00-5:00 size>10 bitrate>=256" },
  { syntax: "key:8A", meaning: "Exactly that key; key:~8A also takes the keys that mix with it (one step round the wheel, relative major / minor). Musical names work: key:Am", example: "key:~8A" },
  { syntax: "tag:peak", meaning: "Has that tag (whole tag, any case; * is a wildcard). Quote tags with spaces", example: "tag:peak tag:\"warm up\" tag:sum*" },
  { syntax: "colour:red", meaning: "Colour label: red orange yellow green teal blue purple pink, or none", example: "colour:red" },
  { syntax: "energy:7-9", meaning: "Energy 1 to 10 under the Energy source setting (your rating, the estimate from the audio, or yours else the estimate). myenergy: is only your rating; autoenergy: only the estimate (autoenergy:7 means 6.5 to 7.5)", example: "energy:7-9 myenergy>=8 autoenergy<4" },
  { syntax: "added:7d", meaning: "Date added / last played: within the last 7d, 2w, 3m, 1y (m = 30 days, y = 365 days); < is newer, > is older. Or a date: 2026, 2026-03, 2026-03-14, >=2026-01-01, 2026-01-01..2026-03-01. played:never", example: "added:30d played:never" },
  { syntax: "is:fav", meaning: "Flags: fav, needs (needs analysis), analysed, unrated, unplayed, untagged", example: "is:fav -is:unplayed" },
  { syntax: "-term", meaning: "Not. Works on words, phrases and every field", example: "-genre:techno -tag:skip" },
  { syntax: "a OR b", meaning: "Either. AND binds tighter, so  x y OR z  means  (x y) OR z", example: "genre:house OR genre:techno" },
];

export const QUERY_SUMMARY = "bpm:120-128 key:~8A tag:peak rating>=4 -genre:techno";
