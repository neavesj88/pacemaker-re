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

export type SortField = "fav" | "title" | "artist" | "album" | "genre" | "bpm" | "key" | "rating" | "times_played" | "play_time_secs" | "colour" | "energy" | "tags";

const collator = new Intl.Collator(undefined, { sensitivity: "base", numeric: true });

// The palette order of state/labels.ts LABEL_COLOURS, repeated here because this file is run by
// plain node in the tests and so cannot import a value through the `$lib` alias; the test file
// checks that the two lists are the same.
export const COLOUR_SORT_ORDER: readonly LabelColour[] = ["red", "orange", "yellow", "green", "teal", "blue", "purple", "pink"];
const colourRank = (c: LabelColour | null) => (c === null ? 999 : COLOUR_SORT_ORDER.indexOf(c));

/** Sorted copy of `rows`. Ties fall back to title then id so the order is stable. The colour,
 *  energy and tags columns sort by the app-side labels (`labelOf`); tracks without that label
 *  come last whichever way the column sorts. */
export function sortTracks(
  rows: Track[],
  field: SortField,
  asc: boolean,
  isFavourite: (id: number) => boolean,
  labelOf: (id: number) => TrackLabels | undefined = () => undefined,
): Track[] {
  const dir = asc ? 1 : -1;
  const keyOf = new Map<number, number>();
  if (field === "key") for (const t of rows) keyOf.set(t.track_id, keyRank(parseKey(t.key)));
  const byLabel = (a: Track, b: Track): number => {
    const x = labelOf(a.track_id);
    const y = labelOf(b.track_id);
    const none = (l: TrackLabels | undefined) => (field === "colour" ? !l?.colour : field === "energy" ? l?.energy == null : !l?.tags.length);
    if (none(x) || none(y)) return none(x) === none(y) ? 0 : (none(x) ? 1 : -1) * dir; // `* dir` cancels the final `* dir`
    if (field === "colour") return colourRank(x!.colour) - colourRank(y!.colour);
    if (field === "energy") return x!.energy! - y!.energy!;
    return collator.compare(x!.tags.join(", "), y!.tags.join(", "));
  };
  const compare = (a: Track, b: Track): number => {
    switch (field) {
      case "colour":
      case "energy":
      case "tags":
        return byLabel(a, b);
      case "fav":
        return Number(isFavourite(a.track_id)) - Number(isFavourite(b.track_id));
      case "key":
        return keyOf.get(a.track_id)! - keyOf.get(b.track_id)!;
      case "bpm":
      case "rating":
      case "times_played":
      case "play_time_secs":
        return a[field] - b[field];
      default:
        return collator.compare(a[field], b[field]);
    }
  };
  return [...rows].sort((a, b) => compare(a, b) * dir || collator.compare(a.title, b.title) || a.track_id - b.track_id);
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
