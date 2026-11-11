// Track matcher: the pure matching module. No DOM, no Tauri; tested by matchLogic.test.mjs.
//
// A request (artist + title as typed) is compared with every library track on normalised text:
// accents, case and punctuation are folded away, "feat." artists and bracketed mix names are
// split off the title, artists are compared as sets. The score is 0-100:
//     100 * (0.6 * title + 0.4 * artist) * version factor
// with caps so that a weak title cannot be rescued by a matching artist (and vice versa), and a
// request with no artist (or a library track with none) never scores above TITLE_ONLY_CAP, so it
// is always put to the user instead of being accepted on the title alone (with the default
// accept threshold). What the score does next (accept, ask, give up) is a setting
// (`MatchOptions`), and the decision the user makes on a row is stored apart from the score.

import type { Track } from "$lib/api/types";
import type { RequestItem } from "./parseList";

// ---- options ---------------------------------------------------------------------------------

/** How a difference in mix name between request and library ("Extended Mix" vs "Radio Edit")
 * counts: strict costs the most score, lenient costs little, ignore costs nothing. */
export type VersionMode = "strict" | "lenient" | "ignore";

export type MatchOptions = {
  /** A best candidate at or above this score is taken as owned without asking (0-100). */
  acceptAt: number;
  /** Below this the request is missing; between this and `acceptAt` the user is asked. */
  reviewAt: number;
  /** Two different tracks both above `acceptAt` and closer than this are ambiguous: asked. 0 = never. */
  margin: number;
  versionMode: VersionMode;
  /** Candidates kept per request for the user to choose from. */
  maxCandidates: number;
};

export const DEFAULT_OPTIONS: MatchOptions = { acceptAt: 85, reviewAt: 60, margin: 3, versionMode: "strict", maxCandidates: 5 };

/** Score cap when the artist is missing on either side. */
export const TITLE_ONLY_CAP = 80;
/** Candidates scoring below this are not offered at all. */
export const SUGGEST_FLOOR = 40;

const clampInt = (v: unknown, lo: number, hi: number, dflt: number) => {
  const n = typeof v === "number" ? v : typeof v === "string" && v.trim() !== "" ? Number(v) : NaN;
  return Number.isFinite(n) ? Math.min(hi, Math.max(lo, Math.round(n))) : dflt;
};

/** Makes any stored or typed value into usable options (out-of-range numbers are clamped,
 * unknown modes fall back to the default, and the review threshold never exceeds the accept one). */
export function cleanOptions(raw: unknown): MatchOptions {
  const r = (raw && typeof raw === "object" ? raw : {}) as Record<string, unknown>;
  const acceptAt = clampInt(r.acceptAt, 1, 100, DEFAULT_OPTIONS.acceptAt);
  const reviewAt = Math.min(acceptAt, clampInt(r.reviewAt, 1, 100, DEFAULT_OPTIONS.reviewAt));
  const mode = r.versionMode === "strict" || r.versionMode === "lenient" || r.versionMode === "ignore" ? r.versionMode : DEFAULT_OPTIONS.versionMode;
  return {
    acceptAt,
    reviewAt,
    margin: clampInt(r.margin, 0, 50, DEFAULT_OPTIONS.margin),
    versionMode: mode,
    maxCandidates: clampInt(r.maxCandidates, 1, 20, DEFAULT_OPTIONS.maxCandidates),
  };
}

// ---- text normalisation ----------------------------------------------------------------------

const SPECIAL: Record<string, string> = { ß: "ss", æ: "ae", œ: "oe", ø: "o", đ: "d", ł: "l", ð: "d", þ: "th", ı: "i" };

/** Lower case, accents and punctuation removed, "&" read as "and", words separated by one space.
 * Apostrophes vanish ("don't" is "dont"); other punctuation separates words. */
export function fold(s: string): string {
  return s
    .normalize("NFKD")
    .replace(/[̀-ͯ]/g, "")
    .toLowerCase()
    .replace(/[ßæœøđłðþı]/g, (c) => SPECIAL[c])
    .replace(/['’‘`´]/g, "")
    .replace(/&/g, " and ")
    .replace(/[^\p{L}\p{N}\p{M}]+/gu, " ")
    .trim()
    .replace(/\s+/g, " ");
}

function levenshtein(a: string, b: string): number {
  if (a === b) return 0;
  if (!a) return b.length;
  if (!b) return a.length;
  let prev = new Uint16Array(b.length + 1);
  let cur = new Uint16Array(b.length + 1);
  for (let j = 0; j <= b.length; j++) prev[j] = j;
  for (let i = 1; i <= a.length; i++) {
    cur[0] = i;
    const ai = a.charCodeAt(i - 1);
    for (let j = 1; j <= b.length; j++) {
      const cost = ai === b.charCodeAt(j - 1) ? 0 : 1;
      cur[j] = Math.min(prev[j] + 1, cur[j - 1] + 1, prev[j - 1] + cost);
    }
    [prev, cur] = [cur, prev];
  }
  return prev[b.length];
}

const MAX_CMP = 160;
const ratio = (a: string, b: string) => {
  a = a.slice(0, MAX_CMP);
  b = b.slice(0, MAX_CMP);
  const n = Math.max(a.length, b.length);
  return n === 0 ? 0 : 1 - levenshtein(a, b) / n;
};
const sortedTokens = (t: string[]) => [...t].sort().join(" ");
const digitTokens = (t: string[]) =>
  t
    .filter((x) => /^\d+$/.test(x))
    .map((x) => x.replace(/^0+(?=\d)/, ""))
    .sort()
    .join(" ");
const letterTokens = (t: string[]) => t.filter((x) => x.length === 1 && !/\d/.test(x)).sort().join(" ");

/** 0-1 similarity of two folded strings: edit distance, also with the words sorted so a
 * different word order does not matter. Numbers must agree ("track 1" is not "track 2"), and
 * when both sides have the same number of words so must single letters ("artist a" is not "artist b"). */
export function similarity(a: string, b: string): number {
  // leading zeros are not a difference: "track 01" is "track 1"
  a = a.replace(/(^| )0+(?=[0-9])/g, "$1");
  b = b.replace(/(^| )0+(?=[0-9])/g, "$1");
  if (!a || !b) return 0;
  if (a === b) return 1;
  const ta = a.split(" ");
  const tb = b.split(" ");
  let s = ratio(a, b);
  if (ta.length > 1 || tb.length > 1) s = Math.max(s, ratio(sortedTokens(ta), sortedTokens(tb)));
  if (digitTokens(ta) !== digitTokens(tb)) s = Math.min(s, 0.55);
  else if (ta.length === tb.length && letterTokens(ta) !== letterTokens(tb)) s = Math.min(s, 0.55);
  return s;
}

// ---- artists and titles ----------------------------------------------------------------------

const ARTIST_SPLIT = /\s*(?:,|;|\/|\+|\s&\s|\sand\s|\sx\s|\s×\s|\svs\.?\s|\sfeat\.?\s|\sft\.?\s|\sfeaturing\s|\swith\s)\s*/i;

/** The individual artist names in a credit, folded, "the" dropped, without repeats. */
export function splitArtists(raw: string): string[] {
  const out: string[] = [];
  for (const part of raw.split(ARTIST_SPLIT)) {
    let n = fold(part);
    if (n.startsWith("the ")) n = n.slice(4);
    if (n && !out.includes(n)) out.push(n);
  }
  return out;
}

const FEAT = /^(?:feat\.?|ft\.?|featuring|with)\s+(.+)$/i;
const FEAT_TAIL = /\s+(?:feat\.?|ft\.?|featuring)\s+(.+)$/i;
// words that make a bracketed or dash-suffixed part a mix name rather than part of the title
const VERSION_WORDS =
  /\b(?:mix|remix|edit|version|dub|vip|rework|bootleg|flip|instrumental|acoustic|live|remaster(?:ed)?|radio|extended|club|reprise|a cappella|acapella|demo|mashup|refix|remode|cut)\b/i;
const NOISE_WORDS = /\b(?:official|video|audio|lyrics?|visuali[sz]er|hd|hq|4k|m\/v|mv|explicit|clean)\b/i;
// words that carry no musical difference and are removed from a mix name before comparing
const VERSION_DROP = new Set([
  "remaster", "remastered", "remastering", "digital", "digitally", "original", "album", "single", "explicit", "clean", "stereo", "mono",
  "official", "video", "audio", "lyric", "lyrics", "visualizer", "visualiser", "hd", "hq", "4k", "mv", "music",
]);

export type TitleParts = {
  /** The title without bracketed mix names, "feat." parts and video tags, folded. */
  core: string;
  /** Artists credited inside the title, folded. */
  feat: string[];
  /** The mix names as written, joined (shown to the user). */
  versionRaw: string;
  /** The mix names that matter, folded ("" for an original mix or a remaster tag). */
  version: string;
};

/** The comparison key of mix names: tags that do not change the recording are dropped, a
 * trailing "mix" / "version" is dropped ("extended mix" and "extended" agree). */
export function versionKey(texts: string[]): string {
  const keys: string[] = [];
  for (const t of texts) {
    const words = fold(t)
      .split(" ")
      .filter((w) => w && !VERSION_DROP.has(w) && !/^(?:19|20)\d\d$/.test(w));
    while (words.length && (words[words.length - 1] === "mix" || words[words.length - 1] === "version")) words.pop();
    if (words.length) keys.push(words.join(" "));
  }
  return keys.join(" ");
}

/** Splits a title into its core, credited guests and mix names. */
export function parseTitle(raw: string): TitleParts {
  const versions: string[] = [];
  const feat: string[] = [];
  let t = raw;
  // bracketed parts: guest credits and mix names are taken out; unlabelled (...) text is part of
  // the title; unlabelled [...] / {...} text (a label, a catalogue number) is dropped
  t = t.replace(/[(\[{]([^()\[\]{}]*)[)\]}]/g, (all, inner: string) => {
    const text = inner.trim();
    const f = FEAT.exec(text);
    if (f) {
      feat.push(...splitArtists(f[1]));
      return " ";
    }
    if (VERSION_WORDS.test(text)) {
      versions.push(text);
      return " ";
    }
    if (NOISE_WORDS.test(text)) return " ";
    return all.startsWith("(") ? ` ${text} ` : " ";
  });
  // "Title - Radio Edit": a dash suffix that names a mix
  const dash = /^(.*\S)\s+[-–—]\s+([^-–—]*)$/.exec(t);
  if (dash && (VERSION_WORDS.test(dash[2]) || NOISE_WORDS.test(dash[2]))) {
    if (VERSION_WORDS.test(dash[2])) versions.push(dash[2].trim());
    t = dash[1];
  }
  const tail = FEAT_TAIL.exec(t);
  if (tail) {
    feat.push(...splitArtists(tail[1]));
    t = t.slice(0, tail.index);
  }
  let core = fold(t);
  if (!core) core = fold(raw);
  return { core, feat, versionRaw: versions.join(" ").trim(), version: versionKey(versions) };
}

/** Everything the scorer needs about one artist + title pair, computed once. */
export type Prepared = {
  titleKey: string;
  /** Folded main artist credit; "" when there is none. */
  artistKey: string;
  /** Every credited artist (main credit and title guests). */
  artists: string[];
  version: string;
  versionRaw: string;
  hasArtist: boolean;
};

export function prepare(artist: string, title: string): Prepared {
  const p = parseTitle(title);
  const main = splitArtists(artist);
  const artists = [...main];
  for (const f of p.feat) if (!artists.includes(f)) artists.push(f);
  return {
    titleKey: p.core,
    artistKey: fold(artist).replace(/^the /, ""),
    artists,
    version: p.version,
    versionRaw: p.versionRaw,
    hasArtist: main.length > 0,
  };
}

function artistScore(a: Prepared, b: Prepared): number {
  const whole = similarity(a.artistKey, b.artistKey);
  const [small, large] = a.artists.length <= b.artists.length ? [a.artists, b.artists] : [b.artists, a.artists];
  if (!small.length) return whole;
  let sum = 0;
  for (const s of small) sum += Math.max(...large.map((l) => similarity(s, l)));
  const coverage = (sum / small.length) * (0.9 + 0.1 * (small.length / large.length));
  return Math.max(whole, coverage);
}

const ONE_SIDED: Record<VersionMode, number> = { strict: 0.9, lenient: 0.97, ignore: 1 };
const DIFFERENT: Record<VersionMode, number> = { strict: 0.75, lenient: 0.9, ignore: 1 };

function versionFactor(a: string, b: string, mode: VersionMode): number {
  if (mode === "ignore" || a === b) return 1;
  if (!a || !b) return ONE_SIDED[mode];
  const base = DIFFERENT[mode];
  return base + (1 - base) * similarity(a, b);
}

export type Scored = { score: number; title: number; artist: number | null; version: number };

/** Scores one request against one library entry. */
export function scorePair(req: Prepared, ent: Prepared, mode: VersionMode): Scored {
  const t = similarity(req.titleKey, ent.titleKey);
  const vf = versionFactor(req.version, ent.version, mode);
  let raw: number;
  let a: number | null = null;
  if (req.hasArtist && ent.hasArtist) {
    a = artistScore(req, ent);
    raw = 0.6 * t + 0.4 * a;
    if (t < 0.6) raw = Math.min(raw, 0.55);
    if (a < 0.5) raw = Math.min(raw, 0.7);
  } else raw = Math.min(t, TITLE_ONLY_CAP / 100);
  raw *= vf;
  // only an exact match reads 100
  const score = raw >= 1 ? 100 : Math.min(99, Math.round(raw * 100));
  return { score, title: t, artist: a, version: vf };
}

// ---- the library index -----------------------------------------------------------------------

/** The parts of a track the matcher reads. `location` is used only as text (a file name to
 * read "Artist - Title" from when the tags have no artist); it is never opened. */
export type MatchTrack = Pick<Track, "track_id" | "title" | "artist" | "location">;

export type LibEntry = Prepared & {
  id: number;
  title: string;
  artist: string;
  /** True for the entry made from the file name of a track with no artist tag. */
  viaFile: boolean;
};

export type Index = {
  entries: LibEntry[];
  byTitle: Map<string, number[]>;
  grams: Map<string, number[]>;
  // scratch space reused by every lookup (a lookup is synchronous)
  counts: Uint16Array;
};

const trigrams = (key: string): string[] => {
  const p = `  ${key} `;
  const out = new Set<string>();
  for (let i = 0; i + 3 <= p.length; i++) out.add(p.slice(i, i + 3));
  return [...out];
};

// a file name "Artist - Title.ext" (kept apart from parseList's splitter: modules here do not
// import each other, so each runs unbundled under node for its tests)
function fromFileName(location: string): { artist: string; title: string } | null {
  const stem = (location.split(/[\\/]/).pop() ?? "").replace(/\.[A-Za-z0-9]{2,5}$/, "");
  const m = /\s+[-–—]\s+/.exec(stem);
  if (!m) return null;
  const artist = stem.slice(0, m.index).trim();
  const title = stem.slice(m.index + m[0].length).trim();
  return artist && title ? { artist, title } : null;
}

export function buildIndex(tracks: readonly MatchTrack[]): Index {
  const entries: LibEntry[] = [];
  const add = (t: MatchTrack, artist: string, title: string, viaFile: boolean) => {
    entries.push({ ...prepare(artist, title), id: t.track_id, title, artist, viaFile });
  };
  for (const t of tracks) {
    add(t, t.artist, t.title, false);
    if (!t.artist.trim()) {
      const f = fromFileName(t.location);
      if (f) add(t, f.artist, f.title, true);
    }
  }
  const byTitle = new Map<string, number[]>();
  const grams = new Map<string, number[]>();
  entries.forEach((e, i) => {
    const same = byTitle.get(e.titleKey);
    if (same) same.push(i);
    else byTitle.set(e.titleKey, [i]);
    for (const g of trigrams(e.titleKey)) {
      const list = grams.get(g);
      if (list) list.push(i);
      else grams.set(g, [i]);
    }
  });
  return { entries, byTitle, grams, counts: new Uint16Array(entries.length) };
}

/** How many entries a lookup scores at most (those sharing the most title trigrams). */
const POOL = 80;

/** Entry indexes worth scoring for a request title: the exact title, plus the entries that
 * share the most trigrams with it. Very common trigrams are skipped unless nothing else is left. */
function pool(index: Index, key: string): number[] {
  const out = new Set<number>(index.byTitle.get(key) ?? []);
  const lists = trigrams(key)
    .map((g) => index.grams.get(g))
    .filter((l): l is number[] => !!l)
    .sort((a, b) => a.length - b.length);
  const limit = Math.max(400, index.entries.length / 8);
  let use = lists.filter((l) => l.length <= limit);
  if (use.length === 0) use = lists.slice(0, 4);
  const touched: number[] = [];
  for (const l of use) {
    for (const i of l) {
      if (index.counts[i]++ === 0) touched.push(i);
    }
  }
  const need = Math.max(1, Math.ceil(0.25 * use.length));
  const ranked = touched.filter((i) => index.counts[i] >= need).sort((a, b) => index.counts[b] - index.counts[a] || a - b);
  for (const i of touched) index.counts[i] = 0;
  for (const i of ranked.slice(0, POOL)) out.add(i);
  return [...out];
}

// ---- matching one request --------------------------------------------------------------------

export type Candidate = {
  track_id: number;
  score: number;
  /** Title and artist similarity 0-1 (artist null when either side has none). */
  title: number;
  artist: number | null;
  /** Said in words when the mix name, the artist or the source of the match needs a look. */
  note: string;
  viaFile: boolean;
};

export type AutoStatus = "owned" | "review" | "missing";

/** What the matcher concludes about one request before the user decides anything. */
export type MatchRow = {
  itemId: number;
  candidates: Candidate[];
  status: AutoStatus;
  /** Why the row needs a look, or how it matched, in words. */
  reason: string;
};

function noteFor(req: Prepared, ent: LibEntry, s: Scored): string {
  const bits: string[] = [];
  if (s.version < 1) bits.push(`mix differs: requested ${req.versionRaw || "no mix name"}, library ${ent.versionRaw || "no mix name"}`);
  if (s.artist === null) bits.push(!req.hasArtist ? "the request has no artist" : "the library track has no artist");
  else if (s.artist < 0.85) bits.push("artist differs");
  if (ent.viaFile) bits.push("matched on the file name");
  return bits.join("; ");
}

/** Matches one request against the library index. `exhaustive` scores every entry (slow; used
 * by the tests to check that the indexed lookup finds the same best match). */
export function matchItem(index: Index, item: Pick<RequestItem, "id" | "artist" | "title">, opts: MatchOptions, exhaustive = false): MatchRow {
  const req = prepare(item.artist, item.title);
  const idxs = exhaustive ? index.entries.map((_, i) => i) : pool(index, req.titleKey);
  const best = new Map<number, Candidate>();
  for (const i of idxs) {
    const ent = index.entries[i];
    const s = scorePair(req, ent, opts.versionMode);
    if (s.score < SUGGEST_FLOOR) continue;
    const prev = best.get(ent.id);
    if (prev && prev.score >= s.score) continue;
    best.set(ent.id, { track_id: ent.id, score: s.score, title: s.title, artist: s.artist, note: noteFor(req, ent, s), viaFile: ent.viaFile });
  }
  const candidates = [...best.values()].sort((a, b) => b.score - a.score || a.track_id - b.track_id).slice(0, opts.maxCandidates);
  const top = candidates[0];
  if (!top || top.score < opts.reviewAt) {
    return { itemId: item.id, candidates, status: "missing", reason: top ? `closest is ${top.score}, below ${opts.reviewAt}` : "nothing similar in the library" };
  }
  if (top.score >= opts.acceptAt) {
    const second = candidates[1];
    if (second && second.score >= opts.acceptAt && top.score - second.score < opts.margin) {
      return { itemId: item.id, candidates, status: "review", reason: "two library tracks match about equally" };
    }
    return { itemId: item.id, candidates, status: "owned", reason: top.score === 100 ? "exact match" : "close match" };
  }
  return { itemId: item.id, candidates, status: "review", reason: top.note || `scored ${top.score}, below ${opts.acceptAt}` };
}

// ---- what the user decided -------------------------------------------------------------------

/** The user's decision on one request: take this library track, or this is not owned. */
export type Choice = { kind: "track"; trackId: number } | { kind: "missing" };

export type Resolved = {
  status: AutoStatus;
  trackId: number | null;
  /** auto: accepted by score; confirmed: the user accepted the best candidate; picked: the user
   * chose another track; rejected: the user said it is not owned; none: still undecided or missing. */
  how: "auto" | "confirmed" | "picked" | "rejected" | "none";
};

export function resolve(row: MatchRow, choice: Choice | undefined): Resolved {
  if (choice?.kind === "missing") return { status: "missing", trackId: null, how: "rejected" };
  if (choice?.kind === "track") {
    return { status: "owned", trackId: choice.trackId, how: row.candidates[0]?.track_id === choice.trackId ? "confirmed" : "picked" };
  }
  if (row.status === "owned") return { status: "owned", trackId: row.candidates[0].track_id, how: "auto" };
  return { status: row.status, trackId: null, how: "none" };
}

export type Counts = { owned: number; review: number; missing: number };

export function countStatuses(rows: readonly MatchRow[], choices: ReadonlyMap<number, Choice> | Record<number, Choice>): Counts {
  const get = (id: number) => (choices instanceof Map ? choices.get(id) : (choices as Record<number, Choice>)[id]);
  const c: Counts = { owned: 0, review: 0, missing: 0 };
  for (const r of rows) c[resolve(r, get(r.itemId)).status]++;
  return c;
}

type ChoiceSource = ReadonlyMap<number, Choice> | Record<number, Choice>;
const choiceOf = (choices: ChoiceSource, id: number) => (choices instanceof Map ? choices.get(id) : (choices as Record<number, Choice>)[id]);

/** The owned requests in request order with the library track each resolves to. The same
 * function feeds the on-screen list, the confirmation and the case that is written. */
export function ownedList(items: readonly RequestItem[], rows: readonly MatchRow[], choices: ChoiceSource): { item: RequestItem; trackId: number; resolved: Resolved; score: number | null }[] {
  const out: { item: RequestItem; trackId: number; resolved: Resolved; score: number | null }[] = [];
  items.forEach((item, i) => {
    const row = rows[i];
    if (!row) return;
    const r = resolve(row, choiceOf(choices, item.id));
    if (r.status !== "owned" || r.trackId === null) return;
    const cand = row.candidates.find((c) => c.track_id === r.trackId);
    out.push({ item, trackId: r.trackId, resolved: r, score: cand ? cand.score : null });
  });
  return out;
}

/** Library track ids for the case: owned requests in request order, a track asked for twice once. */
export function caseTrackIds(owned: readonly { trackId: number }[]): number[] {
  return [...new Set(owned.map((o) => o.trackId))];
}

/** The requests that are missing; with `includeUndecided` also those still waiting for a decision. */
export function missingList(items: readonly RequestItem[], rows: readonly MatchRow[], choices: ChoiceSource, includeUndecided = false): RequestItem[] {
  return items.filter((item, i) => {
    const row = rows[i];
    if (!row) return false;
    const s = resolve(row, choiceOf(choices, item.id)).status;
    return s === "missing" || (includeUndecided && s === "review");
  });
}

/** "Artist - Title" per line (just the title when the request had no artist). */
export function missingAsText(missing: readonly RequestItem[]): string {
  return missing.map((m) => (m.artist ? `${m.artist} - ${m.title}` : m.title)).join("\n") + (missing.length ? "\n" : "");
}

const csvCell = (s: string) => (/[",\r\n]/.test(s) ? `"${s.replace(/"/g, '""')}"` : s);

/** Artist, Title, the line it came from and the line as written. */
export function missingAsCsv(missing: readonly RequestItem[]): string {
  const rows = [["Artist", "Title", "Line", "As written"], ...missing.map((m) => [m.artist, m.title, String(m.line), m.raw])];
  return rows.map((r) => r.map(csvCell).join(",")).join("\r\n") + "\r\n";
}

/** Library tracks whose "artist title" contains every word typed (for the manual pick). */
export function searchTracks<T extends MatchTrack>(tracks: readonly T[], query: string, limit = 100): T[] {
  const words = fold(query).split(" ").filter(Boolean);
  if (!words.length) return [];
  const out: T[] = [];
  for (const t of tracks) {
    const hay = fold(`${t.artist} ${t.title}`);
    if (words.every((w) => hay.includes(w))) {
      out.push(t);
      if (out.length >= limit) break;
    }
  }
  return out;
}
