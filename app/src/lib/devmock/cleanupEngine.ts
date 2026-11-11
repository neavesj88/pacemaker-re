// The metadata clean-up recipes, for the browser mock only. The real engine is Rust
// (src-tauri/src/commands/metadata/cleanup.rs); this is a line-for-line port so `npm run dev`
// shows the same previews, and cleanupGolden.json holds both to the same answers (node test:
// src/lib/views/metadata/cleanup.test.mjs, Rust: cleanup.rs `golden_cases`).
// No $lib imports: the node test loads this file directly.
import type { CleanupSettings, MapEntry } from "../api/metacleanup";

export const TEXT_FIELDS = ["title", "artist", "album_artist", "album", "composer", "genre", "label", "remixer", "producer", "comments"] as const;
export const FEAT_FORMS = ["feat.", "feat", "ft.", "featuring"];

export type Fields = Record<string, string>;

const WS = /\p{White_Space}/u;
const ALNUM = /[\p{Alphabetic}\p{N}]/u;
const ALPHA = /\p{Alphabetic}/u;
const LOWER = /\p{Lowercase}/u;
const UPPER = /\p{Uppercase}/u;
const isWs = (c: string) => WS.test(c);
const isWordChar = (c: string) => ALNUM.test(c);
const isAlpha = (c: string) => ALPHA.test(c);
const chars = (s: string) => Array.from(s);
const hasLetter = (s: string) => chars(s).some(isAlpha);
const isAsciiDigit = (c: string) => c >= "0" && c <= "9";

/** Trims and collapses runs of white space to one space. Only the zero-width space and the byte order
 *  mark are dropped: the zero-width (non-)joiners are part of Persian, Indic and emoji text. */
export function collapseWs(s: string): string {
  let out = "";
  let pending = false;
  for (const c of s) {
    if (c === "\u200b" || c === "\ufeff") continue;
    if (isWs(c)) pending = true;
    else {
      if (pending && out) out += " ";
      pending = false;
      out += c;
    }
  }
  return out;
}

/** collapseWs for one field: a comment may run over several lines, and those line breaks stay; each
 *  line is tidied on its own and blank lines at either end go. Every other field is one line. */
export function tidyField(field: string, s: string): string {
  if (field !== "comments" || !/[\n\r]/.test(s)) return collapseWs(s);
  const lines = s.replace(/\r\n/g, "\n").replace(/\r/g, "\n").split("\n").map(collapseWs);
  const first = lines.findIndex((l) => l !== "");
  let last = -1;
  lines.forEach((l, i) => {
    if (l !== "") last = i;
  });
  return first < 0 ? "" : lines.slice(first, last + 1).join("\n");
}

const wordsLower = (s: string): string[] =>
  s.split(/[^\p{Alphabetic}\p{N}]+/u).filter((w) => w).map((w) => w.toLowerCase());

const isSepChar = (c: string) => "-–—|:/~•·,;".includes(c);

function trimMatches(s: string, pred: (c: string) => boolean): string {
  const cs = chars(s);
  let a = 0;
  let b = cs.length;
  while (a < b && pred(cs[a])) a++;
  while (b > a && pred(cs[b - 1])) b--;
  return cs.slice(a, b).join("");
}

const trimDangling = (s: string) => trimMatches(s, (c) => isWs(c) || isSepChar(c));

// ---- 1. leading track number ----
export function stripTrackNumber(s: string, spaceOnly: boolean, maxDigits: number): string {
  const t = chars(s.replace(/^\p{White_Space}+/u, ""));
  let digits = 0;
  while (digits < t.length && isAsciiDigit(t[digits])) digits++;
  if (digits === 0 || digits > maxDigits) return s;
  const after = t.slice(digits);
  let k = 0;
  while (k < after.length && isWs(after[k])) k++;
  const restWs = after.slice(k);
  const hadSpace = k > 0;
  const sep = restWs[0];
  if (sep !== undefined && "-–—._):".includes(sep)) {
    const afterSep = restWs.slice(1);
    const spacedAfter = afterSep.length > 0 && isWs(afterSep[0]);
    let j = 0;
    while (j < afterSep.length && (isWs(afterSep[j]) || afterSep[j] === "_")) j++;
    const rest = afterSep.slice(j).join("");
    const gluedOk = digits >= 2 || hadSpace || spacedAfter;
    const decimal = sep === "." && rest !== "" && isAsciiDigit(chars(rest)[0]);
    return gluedOk && !decimal && hasLetter(rest) ? rest : s;
  }
  if (sep !== undefined && spaceOnly && hadSpace && digits === 2) {
    const rest = restWs.join("");
    return hasLetter(rest) && !isAsciiDigit(restWs[0]) ? rest : s;
  }
  return s;
}

// ---- 2. URLs and promo text ----
const TLDS = [
  "com", "net", "org", "info", "biz", "ru", "fm", "co", "io", "me", "tv", "to", "ws", "cc", "us", "uk", "de", "nl",
  "fr", "it", "es", "pl", "se", "ch", "at", "be", "dk", "no", "fi", "jp", "cn", "br", "au", "nz",
];

/** Top-level domains that are also ordinary words ("Hello.It", "Say.No"): a bare address ending in one
 *  of them counts only as the whole content of a bracket, where promo text lives. */
const WORD_TLDS = ["it", "me", "to", "us", "no", "at", "be", "es"];

/** Is this one white-space-free token a web address? "http://...", "https://...", anything with "://"
 *  and "www.x.y" always; a bare "name.tld" or "name.tld/path" only when it is written in lower case, as
 *  addresses are ("Rock.It", "Dance.With.Me" and "Big Beat.Net" are titles), has a name of at least
 *  three characters, a known top-level domain and no more than one dot unless a path follows; a bare
 *  address on a word-like domain only counts inBracket. */
function looksLikeUrl(token: string, inBracket: boolean): boolean {
  let t = trimMatches(token, (c) => !ALNUM.test(c) && c !== "/" && c !== ".");
  while (t.endsWith(".")) t = t.slice(0, -1);
  const lower = t.toLowerCase();
  if (lower.startsWith("http://") || lower.startsWith("https://") || lower.startsWith("www.") || lower.includes("://")) return true;
  if (t !== lower) return false;
  const slash = t.indexOf("/");
  const host = slash < 0 ? t : t.slice(0, slash);
  const path = slash < 0 ? "" : t.slice(slash + 1);
  const hasPath = chars(path).some((c) => ALNUM.test(c));
  const parts = host.split(".");
  if (parts.length < 2 || parts.some((x) => x === "") || (parts.length > 2 && !hasPath)) return false;
  const tld = parts[parts.length - 1];
  const label = parts[parts.length - 2];
  if (!TLDS.includes(tld) || chars(label).length < 3 || !chars(label).every((c) => ALNUM.test(c) || c === "-")) return false;
  return hasPath || inBracket || !WORD_TLDS.includes(tld);
}

const splitWs = (s: string): string[] => {
  const out: string[] = [];
  let cur = "";
  for (const c of s) {
    if (isWs(c)) {
      if (cur) out.push(cur);
      cur = "";
    } else cur += c;
  }
  if (cur) out.push(cur);
  return out;
};

const containsUrl = (text: string, inBracket: boolean) => splitWs(text).some((t) => looksLikeUrl(t, inBracket));

function isPromo(content: string, phrases: string[][]): boolean {
  // a bracket holding nothing but one address ("[djsite.me]") is promo even on a word-like domain
  if (containsUrl(content, splitWs(content).length === 1)) return true;
  // promo phrases only count when they are the whole bracket ("(Exclusive Remix)" is kept)
  const words = wordsLower(content);
  return words.length > 0 && coveredByPhrases(words, phrases);
}

function coveredByPhrases(words: string[], phrases: string[][]): boolean {
  let i = 0;
  while (i < words.length) {
    let best = 0;
    for (const p of phrases) {
      if (p.length > best && p.every((w, j) => words[i + j] === w)) best = p.length;
    }
    if (best === 0) return false;
    i += best;
  }
  return true;
}

const CLOSERS: Record<string, string> = { "(": ")", "[": "]", "{": "}" };

/** `[contentStart, contentEnd, indexAfterCloser]` of the group opening at `i`, or null. */
function findGroup(cs: string[], i: number): [number, number, number] | null {
  const open = cs[i];
  const close = CLOSERS[open];
  if (!close) return null;
  let depth = 0;
  for (let j = i; j < cs.length; j++) {
    if (cs[j] === open) depth++;
    else if (cs[j] === close) {
      depth--;
      if (depth === 0) return [i + 1, j, j + 1];
    }
  }
  return null;
}

function stripBracketPromo(s: string, phrases: string[][]): string {
  const cs = chars(s);
  let out = "";
  let i = 0;
  while (i < cs.length) {
    const c = cs[i];
    const g = findGroup(cs, i);
    if (g) {
      const [a, b, next] = g;
      const inner = cs.slice(a, b).join("");
      const cleaned = stripBracketPromo(inner, phrases);
      if (isPromo(cleaned, phrases)) {
        while (out.endsWith(" ")) out = out.slice(0, -1);
        i = next;
        continue;
      }
      out += c + (cleaned === inner ? cleaned : cleaned.replace(/\p{White_Space}+$/u, "")) + cs[b];
      i = next;
      continue;
    }
    out += c;
    i++;
  }
  return out;
}

export function stripPromo(s: string, phraseList: string[]): string {
  const phrases = phraseList.map(wordsLower).filter((p) => p.length > 0);
  const noBrackets = stripBracketPromo(s, phrases);
  const kept = splitWs(noBrackets).filter((t) => !looksLikeUrl(t, false));
  const joined = kept.join(" ");
  if (joined === collapseWs(s)) return s;
  return trimDangling(joined);
}

/** stripPromo for one field. A comment is judged line by line, so its line breaks stay and a line that
 *  was nothing but promo disappears whole. */
export function stripPromoField(field: string, s: string, phraseList: string[]): string {
  if (field !== "comments" || !s.includes("\n")) return stripPromo(s, phraseList);
  return s
    .split("\n")
    .flatMap((line) => {
      const cleaned = stripPromo(line, phraseList);
      return cleaned.trim() === "" && line.trim() !== "" ? [] : [cleaned];
    })
    .join("\n");
}

// ---- 3. feat. forms ----
const isFeatWord = (w: string) => ["feat", "ft", "featuring"].includes(w.toLowerCase());

/** `[start, endOfWord, endIncludingDot]` of each feat token. */
function featTokens(cs: string[]): [number, number, number][] {
  const out: [number, number, number][] = [];
  let i = 1;
  while (i < cs.length) {
    const prev = cs[i - 1];
    if ((isWs(prev) || "([{".includes(prev)) && isAlpha(cs[i])) {
      let j = i;
      while (j < cs.length && isAlpha(cs[j])) j++;
      const word = cs.slice(i, j).join("");
      if (isFeatWord(word)) {
        const dot = j < cs.length && cs[j] === ".";
        const end = dot ? j + 1 : j;
        const followsOk = end < cs.length && (isWs(cs[end]) || (dot && isWordChar(cs[end])));
        if (followsOk) {
          out.push([i, j, end]);
          i = end;
          continue;
        }
      }
      i = j;
      continue;
    }
    i++;
  }
  return out;
}

export function normaliseFeat(s: string, form: string): string {
  const cs = chars(s);
  const tokens = featTokens(cs);
  if (tokens.length === 0) return s;
  let out = "";
  let at = 0;
  for (const [start, , end] of tokens) {
    out += cs.slice(at, start).join("") + form;
    if (end < cs.length && !isWs(cs[end])) out += " ";
    at = end;
  }
  return out + cs.slice(at).join("");
}

function bracketDepth(cs: string[]): number {
  let depth = 0;
  for (const c of cs) {
    if ("([{".includes(c)) depth++;
    else if (")]}".includes(c)) depth = Math.max(0, depth - 1);
  }
  return depth;
}

export function splitFeat(title: string): [string, string] | null {
  const cs = chars(title);
  let i = 0;
  while (i < cs.length) {
    const g = findGroup(cs, i);
    if (g) {
      const [a, b, next] = g;
      const inner = cs.slice(a, b).join("");
      const probe = chars(" " + inner.replace(/^\p{White_Space}+|\p{White_Space}+$/gu, ""));
      const first = featTokens(probe)[0];
      if (first) {
        const [start, , end] = first;
        const names = collapseWs(probe.slice(end).join(""));
        if (start === 1 && names) {
          let left = cs.slice(0, i).join("");
          while (left.endsWith(" ")) left = left.slice(0, -1);
          const right = cs.slice(next).join("");
          return [collapseWs(`${left} ${right}`), names];
        }
      }
      i = next;
      continue;
    }
    i++;
  }
  // only a token outside every bracket; inside "(Foo feat. Bar Remix)" cutting would unbalance it
  const found = featTokens(cs).find(([s]) => bracketDepth(cs.slice(0, s)) === 0);
  if (!found) return null;
  const [start, , end] = found;
  const rest = cs.slice(end);
  let stop = rest.findIndex((c) => "([{".includes(c));
  if (stop < 0) stop = rest.length;
  const names = trimDangling(collapseWs(rest.slice(0, stop).join("")));
  if (!names) return null;
  const left = cs.slice(0, start).join("");
  const right = rest.slice(stop).join("");
  return [collapseWs(`${left} ${right}`), names];
}

// ---- 4. remixer ----
export function extractRemixer(title: string, remixWords: string[], ignoreWords: string[]): string | null {
  const cs = chars(title);
  const remix = remixWords.map((w) => w.toLowerCase());
  const ignore = ignoreWords.map((w) => w.toLowerCase());
  const bare = (w: string) => trimMatches(w, (c) => !ALNUM.test(c)).toLowerCase();
  let i = 0;
  while (i < cs.length) {
    const g = findGroup(cs, i);
    if (!g) {
      i++;
      continue;
    }
    const [a, b, next] = g;
    const inner = cs.slice(a, b).join("");
    i = next;
    const tokens = splitWs(inner);
    if (tokens.length === 0) continue;
    const last = tokens[tokens.length - 1];
    if (!remix.includes(bare(last))) continue;
    const names = tokens.slice(0, -1);
    while (names.length > 0 && ignore.includes(bare(names[names.length - 1]))) names.pop();
    if (names.length === 0) continue;
    let name = names.join(" ");
    for (const suffix of ["'s", "’s"]) if (name.endsWith(suffix)) name = name.slice(0, -suffix.length);
    name = name.trim();
    if (hasLetter(name)) return name;
  }
  return null;
}

// ---- 5. Title Case ----
const isSmallWord = (w: string, small: string[]) => small.some((x) => x.toLowerCase() === w.toLowerCase());

function capitalise(w: string): string {
  let out = "";
  let done = false;
  for (const c of w) {
    if (!done && isAlpha(c)) {
      out += c.toUpperCase();
      done = true;
    } else if (done) out += c.toLowerCase();
    else out += c;
  }
  return out;
}

type Shape = "none" | "lower" | "upper" | "cap" | "mixed";
function shape(w: string): Shape {
  const letters = chars(w).filter(isAlpha);
  if (letters.length === 0) return "none";
  if (letters.every((c) => LOWER.test(c))) return "lower";
  if (letters.every((c) => UPPER.test(c))) return "upper";
  if (UPPER.test(letters[0]) && letters.slice(1).every((c) => LOWER.test(c))) return "cap";
  return "mixed";
}

const isApostrophe = (c: string) => c === "'" || c === "’";

export function titleCase(s: string, keepUpper: string[], smallWords: string[]): string {
  const keep = new Map(keepUpper.map((k) => [k.toLowerCase(), k] as const));
  const letters = chars(s).filter(isAlpha);
  const allUpper = letters.length > 0 && letters.every((c) => !LOWER.test(c));
  const cs = chars(s);
  const wordStartAt = (i: number) => isWordChar(cs[i]) || (isApostrophe(cs[i]) && i + 1 < cs.length && isWordChar(cs[i + 1]));
  const tokens: [boolean, string][] = [];
  let i = 0;
  while (i < cs.length) {
    if (wordStartAt(i)) {
      let j = i + 1;
      while (j < cs.length) {
        const d = cs[j];
        const apostropheInside = isApostrophe(d) && j + 1 < cs.length && isWordChar(cs[j + 1]);
        if (isWordChar(d) || apostropheInside) j++;
        else break;
      }
      let text = cs.slice(i, j).join("");
      if (j < cs.length && cs[j] === ".") {
        const l = text.toLowerCase();
        if (l === "feat" || l === "ft" || l === "vs") {
          text += ".";
          j++;
        }
      }
      tokens.push([true, text]);
      i = j;
    } else {
      let j = i + 1;
      while (j < cs.length && !wordStartAt(j)) j++;
      tokens.push([false, cs.slice(i, j).join("")]);
      i = j;
    }
  }
  // the first and last word count numbers too: "3 of 4" keeps its "of" small
  const allWords = tokens.flatMap(([w], idx) => (w ? [idx] : []));
  const lastWord = allWords[allWords.length - 1];
  const firstWord = allWords[0];
  let out = "";
  let prevSep = "";
  tokens.forEach(([isWord, text], idx) => {
    if (!isWord) {
      out += text;
      prevSep = text;
      return;
    }
    if (!hasLetter(text)) {
      out += text;
      return;
    }
    let base = text;
    let tail = "";
    for (const suffix of ["'s", "’s"]) {
      if (text.endsWith(suffix)) {
        base = text.slice(0, -suffix.length);
        tail = suffix;
        break;
      }
    }
    const spelling = keep.get(base.toLowerCase());
    if (spelling !== undefined) {
      out += spelling + tail;
      return;
    }
    const sepChars = chars(prevSep);
    const clauseStart =
      idx === firstWord ||
      idx === lastWord ||
      sepChars.some((c) => "([{:\"“!?".includes(c)) ||
      prevSep.includes(" - ") ||
      prevSep.includes(" – ") ||
      prevSep.includes(" — ") ||
      prevSep.includes(". ") ||
      (prevSep !== "" && !sepChars.some(isWs));
    const sh = shape(text);
    const letterCount = chars(text).filter(isAlpha).length;
    let converted: string;
    if (isAsciiDigit(chars(text)[0])) converted = text;
    else if (sh === "none" || sh === "mixed") converted = text;
    else if (sh === "upper" && !allUpper && letterCount > 1) converted = text;
    else if (sh === "upper" && !allUpper) converted = capitalise(text);
    else if (!clauseStart && isSmallWord(text, smallWords)) converted = text.toLowerCase();
    else if (sh === "cap") converted = text;
    else converted = capitalise(text);
    out += converted;
  });
  return out;
}

// ---- 6. maps ----
function lookup(map: MapEntry[], value: string): string | null {
  const key = collapseWs(value).toLowerCase();
  const hit = map.find((e) => collapseWs(e.from).toLowerCase() === key);
  return hit ? hit.to : null;
}

/** The text each separator of the settings stands for, longest first: a comma (or semicolon) goes
 *  straight after a name, anything else has a space on both sides. */
function separatorPatterns(separators: string[]): string[][] {
  return separators
    .filter((x) => x !== "")
    .map((x) => {
      const l = x.toLowerCase();
      return chars(l === "," || l === ";" ? `${l} ` : ` ${l} `);
    })
    .sort((a, b) => b.length - a.length);
}

function splitArtists(s: string, separators: string[]): [string, string][] {
  const patterns = separatorPatterns(separators);
  const cs = chars(s);
  const lower = chars(s.toLowerCase());
  if (lower.length !== cs.length) return [[s, ""]];
  const parts: [string, string][] = [];
  let start = 0;
  let i = 0;
  while (i < cs.length) {
    let hit = 0;
    for (const sc of patterns) {
      if (i + sc.length <= cs.length && sc.every((c, k) => lower[i + k] === c)) {
        hit = sc.length;
        break;
      }
    }
    if (hit > 0) {
      parts.push([cs.slice(start, i).join(""), cs.slice(i, i + hit).join("")]);
      i += hit;
      start = i;
    } else i++;
  }
  parts.push([cs.slice(start).join(""), ""]);
  return parts;
}

function applyArtistMap(value: string, map: MapEntry[], separators: string[]): string {
  const whole = lookup(map, value);
  if (whole !== null) return whole;
  const parts = splitArtists(value, separators);
  if (parts.length === 1) return value;
  return parts.map(([name, sep]) => (lookup(map, name) ?? name) + sep).join("");
}

// ---- the pipeline ----
export function cleanFields(cur: Fields, s: CleanupSettings): Fields {
  const w: Fields = { ...cur };
  const get = (f: string) => w[f] ?? "";
  const edit = (f: string, v: string) => {
    if (v !== get(f)) w[f] = tidyField(f, v);
  };
  if (s.trim) for (const f of TEXT_FIELDS) w[f] = tidyField(f, get(f));
  if (s.strip_promo) for (const f of s.promo_fields) edit(f, stripPromoField(f, get(f), s.promo_phrases));
  if (s.strip_number) edit("title", stripTrackNumber(get("title"), s.number_space_only, s.number_max_digits));
  if (s.feat_normalise) for (const f of ["title", "artist", "album_artist"]) edit(f, normaliseFeat(get(f), s.feat_form));
  if (s.feat_to_artist) {
    const artist = get("artist");
    if (artist) {
      const split = splitFeat(get("title"));
      if (split && split[0]) {
        const [title, names] = split;
        const form = s.feat_normalise ? s.feat_form : "feat.";
        w.title = title;
        if (!artist.toLowerCase().includes(names.toLowerCase())) w.artist = `${artist} ${form} ${names}`;
      }
    }
  }
  if (s.extract_remixer) {
    const name = extractRemixer(get("title"), s.remix_words, s.remixer_ignore);
    if (name !== null && (get("remixer") === "" || s.overwrite_remixer)) w.remixer = name;
  }
  if (s.title_case) for (const f of s.case_fields) edit(f, titleCase(get(f), s.keep_upper, s.small_words));
  if (s.genre_map_on) {
    const to = lookup(s.genre_map, get("genre"));
    if (to !== null) w.genre = to;
  }
  if (s.artist_map_on) for (const f of ["artist", "album_artist"]) edit(f, applyArtistMap(get(f), s.artist_map, s.artist_separators));
  if (s.trim) for (const f of TEXT_FIELDS) w[f] = tidyField(f, get(f));
  return w;
}

export const MAX_TEXT_CHARS = 255;
export const problemOf = (field: string, old: string, v: string): string | null => {
  if ((field === "title" || field === "artist") && v.trim() === "" && old.trim() !== "") return `${field} would be left empty`;
  if (chars(v).length > MAX_TEXT_CHARS) return `${field} would be longer than ${MAX_TEXT_CHARS} characters`;
  if (v.includes("\0")) return `${field} would contain a NUL character`;
  return null;
};

