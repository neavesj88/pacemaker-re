// Pure logic of the Tracks view: musical key -> Camelot, key compatibility, filters, sorting and
// "matching tracks". No store or component imports (only types) so it can be tested with plain
// node: `node --experimental-strip-types --test src/lib/views/tracks/trackLogic.test.mjs`.
import type { Track } from "$lib/api/types";
import type { LabelColour, TrackLabels } from "$lib/state/labels";

// ---- keys ---------------------------------------------------------------------------------

export type Camelot = { num: number; letter: "A" | "B"; code: string };

const NOTE_PC: Record<string, number> = { c: 0, d: 2, e: 4, f: 5, g: 7, a: 9, b: 11 };
const FLAT_MAJOR = ["C", "Db", "D", "Eb", "E", "F", "F#", "G", "Ab", "A", "Bb", "B"];
const FLAT_MINOR = ["C", "C#", "D", "Eb", "E", "F", "F#", "G", "G#", "A", "Bb", "B"];

const make = (num: number, letter: "A" | "B"): Camelot => ({ num, letter, code: `${num}${letter}` });

/** Camelot number of a major key by pitch class (C = 8B, G = 9B, ..., F = 7B). */
const majorNumber = (pc: number) => (((pc * 7) % 12) + 7) % 12 + 1;

/** Reads a stored musical key ("Am", "F#m", "C", "Ab Minor", "Db major", "8A") as a Camelot
 *  code, or null when empty or not recognised. */
export function parseKey(key: string): Camelot | null {
  const text = key.replace(/♯/g, "#").replace(/♭/g, "b").trim();
  if (!text) return null;
  const direct = /^(1[0-2]|[1-9])\s*([ABab])$/.exec(text);
  if (direct) return make(Number(direct[1]), direct[2].toUpperCase() as "A" | "B");

  const m = /^([A-Ga-g])\s*([#b]?)\s*(.*)$/.exec(text);
  if (!m) return null;
  let pc = NOTE_PC[m[1].toLowerCase()];
  if (m[2] === "#") pc += 1;
  if (m[2] === "b") pc -= 1;
  pc = (pc + 12) % 12;
  const rest = m[3].trim();
  let minor: boolean;
  if (rest === "" || rest === "M" || /^maj(or)?$/i.test(rest)) minor = false;
  else if (rest === "m" || /^min(or)?$/i.test(rest)) minor = true;
  else return null;
  // a minor key shares its number with the relative major, three semitones up
  return minor ? make(majorNumber((pc + 3) % 12), "A") : make(majorNumber(pc), "B");
}

/** "A minor" / "Db major" for a Camelot key; the name the user would say aloud. */
export function keyName(c: Camelot): string {
  for (let pc = 0; pc < 12; pc++) {
    if (majorNumber(pc) !== c.num) continue;
    return c.letter === "B" ? `${FLAT_MAJOR[pc]} major` : `${FLAT_MINOR[(pc + 9) % 12]} minor`;
  }
  return c.code;
}

/** Same key, one step round the wheel, or the relative major / minor. */
export function keysCompatible(a: Camelot, b: Camelot): boolean {
  if (a.num === b.num) return true;
  if (a.letter !== b.letter) return false;
  const d = Math.abs(a.num - b.num);
  return d === 1 || d === 11;
}

/** Hue-per-number colour for a key badge; keys that mix well sit next to each other. */
export const keyColour = (c: Camelot) => `hsl(${(c.num * 30 + 300) % 360} 70% 62%)`;

/** Sort position of a key: 1A, 1B, 2A ...; unknown keys last. */
export const keyRank = (c: Camelot | null) => (c ? c.num * 2 + (c.letter === "B" ? 1 : 0) : 999);

export const ALL_CAMELOT: Camelot[] = Array.from({ length: 12 }, (_, i) => [make(i + 1, "A"), make(i + 1, "B")]).flat();

// ---- filters ------------------------------------------------------------------------------

/** "" (any), "compat:8A" (compatible with 8A) or "is:8A" (exactly 8A). */
export type KeyFilter = string;

export type TrackFilter = {
  genre: string;
  bpmMin: number | null;
  bpmMax: number | null;
  key: KeyFilter;
  /** Only tracks that do not need analysis. */
  onlyAnalysed: boolean;
};

export const NO_FILTER: TrackFilter = { genre: "", bpmMin: null, bpmMax: null, key: "", onlyAnalysed: false };

/** A BPM box's text as a number, or null when blank or not a positive number. */
export function parseBpm(text: string): number | null {
  const n = Number(text.trim().replace(",", "."));
  return text.trim() !== "" && Number.isFinite(n) && n > 0 ? n : null;
}

export function filterIsActive(f: TrackFilter): boolean {
  return !!f.genre || f.bpmMin !== null || f.bpmMax !== null || !!f.key || f.onlyAnalysed;
}

export function filterTracks(list: Track[], f: TrackFilter, needs: ReadonlyMap<number, string>): Track[] {
  const [mode, code] = f.key.split(":");
  const wanted = code ? parseKey(code) : null;
  const bpmBound = f.bpmMin !== null || f.bpmMax !== null;
  return list.filter((t) => {
    if (f.genre && t.genre !== f.genre) return false;
    if (f.onlyAnalysed && needs.has(t.track_id)) return false;
    if (bpmBound) {
      // the table shows no BPM for a track that needs analysis, so it has none to match
      if (!(t.bpm > 0) || needs.has(t.track_id)) return false;
      if (f.bpmMin !== null && t.bpm < f.bpmMin) return false;
      if (f.bpmMax !== null && t.bpm > f.bpmMax) return false;
    }
    if (wanted) {
      const k = parseKey(t.key);
      if (!k) return false;
      if (mode === "is" ? k.code !== wanted.code : !keysCompatible(wanted, k)) return false;
    }
    return true;
  });
}

export function uniqueGenres(list: Track[]): string[] {
  return [...new Set(list.map((t) => t.genre).filter(Boolean))].sort((a, b) => a.localeCompare(b, undefined, { sensitivity: "base" }));
}

// ---- sorting ------------------------------------------------------------------------------

export type SortField =
  | "fav" | "title" | "artist" | "album" | "genre" | "bpm" | "key" | "rating" | "times_played" | "play_time_secs" | "colour" | "energy" | "tags"
  | "year" | "label" | "remixer" | "composer" | "comments" | "date_added" | "last_played" | "format" | "bit_rate" | "file_size";

/** One key of a sort: the column and its direction. */
export type SortSpec = { field: SortField; asc: boolean };

const collator = new Intl.Collator(undefined, { sensitivity: "base", numeric: true });

// The palette order of state/labels.ts LABEL_COLOURS, repeated here because this file is run by
// plain node in the tests and so cannot import a value through the `$lib` alias; the test file
// checks that the two lists are the same.
export const COLOUR_SORT_ORDER: readonly LabelColour[] = ["red", "orange", "yellow", "green", "teal", "blue", "purple", "pink"];
const colourRank = (c: LabelColour | null) => (c === null ? 999 : COLOUR_SORT_ORDER.indexOf(c));

/** Number fields whose "nothing here" value (no year, never played ...) sorts after every real
 *  value in both directions, like the app-side label columns, as do the empty label, remixer,
 *  composer, comments and format. The older columns keep the order they always had (an empty
 *  artist sorts first ascending). */
const MISSING_LAST_NUMBER: Partial<Record<SortField, (t: Track) => number | null>> = {
  year: (t) => (t.year > 0 ? t.year : null),
  date_added: (t) => (t.date_added > 0 ? t.date_added : null),
  last_played: (t) => (t.last_played > 0 ? t.last_played : null),
  bit_rate: (t) => (t.bit_rate > 0 ? t.bit_rate : null),
};

/** Sorted copy of `rows` by the keys in order (the first is the primary key). Ties fall back to
 *  title then id so the order is stable. The colour, energy and tags columns sort by the
 *  app-side labels (`labelOf`); tracks without that label come last whichever way the column
 *  sorts. An empty `keys` sorts by title. */
export function sortTracksBy(
  rows: Track[],
  keys: readonly SortSpec[],
  isFavourite: (id: number) => boolean,
  labelOf: (id: number) => TrackLabels | undefined = () => undefined,
): Track[] {
  const comparators = keys.map((k) => comparatorFor(k, rows, isFavourite, labelOf));
  return [...rows].sort((a, b) => {
    for (const c of comparators) {
      const r = c(a, b);
      if (r) return r;
    }
    return collator.compare(a.title, b.title) || a.track_id - b.track_id;
  });
}

/** One column's comparison, already turned the way `asc` says. */
function comparatorFor(
  spec: SortSpec,
  rows: Track[],
  isFavourite: (id: number) => boolean,
  labelOf: (id: number) => TrackLabels | undefined,
): (a: Track, b: Track) => number {
  const { field } = spec;
  const dir = spec.asc ? 1 : -1;
  /** Compares two values that may be missing; missing ones come last whichever way it sorts. */
  const withMissing = <V,>(get: (t: Track) => V | null, cmp: (x: V, y: V) => number) => (a: Track, b: Track): number => {
    const x = get(a);
    const y = get(b);
    if (x === null || y === null) return x === y ? 0 : x === null ? 1 : -1;
    return cmp(x, y) * dir;
  };
  switch (field) {
    case "colour": return withMissing((t) => labelOf(t.track_id)?.colour ?? null, (x, y) => colourRank(x) - colourRank(y));
    case "energy": return withMissing((t) => labelOf(t.track_id)?.energy ?? null, (x, y) => x - y);
    case "tags": return withMissing((t) => { const l = labelOf(t.track_id)?.tags; return l && l.length > 0 ? l.join(", ") : null; }, (x, y) => collator.compare(x, y));
    case "fav": return (a, b) => (Number(isFavourite(a.track_id)) - Number(isFavourite(b.track_id))) * dir;
    case "key": {
      const rank = new Map<number, number>();
      for (const t of rows) rank.set(t.track_id, keyRank(parseKey(t.key)));
      return (a, b) => (rank.get(a.track_id)! - rank.get(b.track_id)!) * dir;
    }
    case "bpm":
    case "rating":
    case "times_played":
    case "play_time_secs":
    case "file_size":
      return (a, b) => (a[field] - b[field]) * dir;
    case "year":
    case "date_added":
    case "last_played":
    case "bit_rate": {
      const get = MISSING_LAST_NUMBER[field]!;
      return withMissing(get, (x, y) => x - y);
    }
    case "label":
    case "remixer":
    case "composer":
    case "comments":
    case "format":
      return withMissing((t) => (t[field].trim() === "" ? null : t[field]), (x, y) => collator.compare(x, y));
    default:
      return (a, b) => collator.compare(a[field], b[field]) * dir;
  }
}

/** Sorted copy of `rows` by one column; see {@link sortTracksBy}. */
export function sortTracks(
  rows: Track[],
  field: SortField,
  asc: boolean,
  isFavourite: (id: number) => boolean,
  labelOf: (id: number) => TrackLabels | undefined = () => undefined,
): Track[] {
  return sortTracksBy(rows, [{ field, asc }], isFavourite, labelOf);
}

/** The sort after a click on a column header. A plain click makes the column the only key (or, if
 *  it already is the primary key, reverses it and keeps the secondary keys); a shift-click adds
 *  the column as the last key, then reverses it, then removes it (a lone key is just reversed). */
export function sortClick(keys: readonly SortSpec[], field: SortField, additive: boolean): SortSpec[] {
  const i = keys.findIndex((k) => k.field === field);
  if (!additive) {
    if (i === 0) return keys.map((k, j) => (j === 0 ? { field, asc: !k.asc } : { ...k }));
    return [{ field, asc: true }];
  }
  if (i < 0) return [...keys.map((k) => ({ ...k })), { field, asc: true }];
  if (keys[i].asc) return keys.map((k, j) => (j === i ? { field, asc: false } : { ...k }));
  if (keys.length === 1) return [{ field, asc: true }];
  return keys.filter((_, j) => j !== i).map((k) => ({ ...k }));
}

// ---- cell text ------------------------------------------------------------------------------

const pad2 = (n: number) => String(n).padStart(2, "0");

/** A time in Unix milliseconds as a local date, `2026-03-14`; empty when there is none. */
export function formatDate(ms: number): string {
  if (!(ms > 0)) return "";
  const d = new Date(ms);
  return `${d.getFullYear()}-${pad2(d.getMonth() + 1)}-${pad2(d.getDate())}`;
}

/** A file size as `7.6 MB`; empty for no size. */
export function formatSize(bytes: number): string {
  if (!(bytes > 0)) return "";
  const units = ["B", "KB", "MB", "GB", "TB"];
  let v = bytes;
  let u = 0;
  while (v >= 1024 && u < units.length - 1) {
    v /= 1024;
    u++;
  }
  return `${u === 0 ? v : v.toFixed(v >= 100 ? 0 : 1)} ${units[u]}`;
}

// ---- matching tracks ----------------------------------------------------------------------

/** Tracks that mix well with `focus`: compatible key (same, +-1 step, relative) and BPM within
 *  +-`bpmPct` percent. Closest BPM first. Needs a known key and BPM on `focus`. */
export function matchingTracks(focus: Track, list: Track[], bpmPct = 4, needs: ReadonlyMap<number, string> = new Map()): Track[] {
  const k = parseKey(focus.key);
  if (!k || !(focus.bpm > 0)) return [];
  const span = (focus.bpm * bpmPct) / 100;
  return list
    .filter((t) => {
      if (t.track_id === focus.track_id || !(t.bpm > 0) || needs.has(t.track_id)) return false;
      if (Math.abs(t.bpm - focus.bpm) > span) return false;
      const tk = parseKey(t.key);
      return !!tk && keysCompatible(k, tk);
    })
    .sort((a, b) => Math.abs(a.bpm - focus.bpm) - Math.abs(b.bpm - focus.bpm) || a.track_id - b.track_id);
}
