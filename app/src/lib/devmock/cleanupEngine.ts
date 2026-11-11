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

/** Trims and collapses runs of white space to one space; zero-width characters are dropped. */
export function collapseWs(s: string): string {
  let out = "";
  let pending = false;
  for (const c of s) {
    if (c === "​" || c === "﻿" || c === "‌" || c === "‍") continue;
    if (isWs(c)) pending = true;
    else {
      if (pending && out) out += " ";
      pending = false;
      out += c;
    }
  }
  return out;
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
export function stripTrackNumber(s: string, spaceOnly: boolean): string {
  const t = chars(s.replace(/^\p{White_Space}+/u, ""));
  let digits = 0;
  while (digits < t.length && isAsciiDigit(t[digits])) digits++;
  if (digits === 0 || digits > 3) return s;
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

function looksLikeUrl(token: string): boolean {
  const t = trimMatches(token, (c) => !ALNUM.test(c) && c !== "/" && c !== ".").toLowerCase();
  if (t.startsWith("http://") || t.startsWith("https://") || t.startsWith("www.") || t.includes("://")) return true;
  const host = t.split("/")[0];
  const parts = host.split(".");
  if (parts.length < 2) return false;
  const tld = parts[parts.length - 1];
  const label = parts[parts.length - 2];
  return TLDS.includes(tld) && chars(label).length >= 3 && chars(label).every((c) => ALNUM.test(c) || c === "-");
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

const containsUrl = (text: string) => splitWs(text).some(looksLikeUrl);

function phraseIn(words: string[], phrase: string[]): boolean {
  if (phrase.length === 0) return false;
  for (let i = 0; i + phrase.length <= words.length; i++) {
    if (phrase.every((p, j) => words[i + j] === p)) return true;
  }
  return false;
}

function isPromo(content: string, phrases: string[][]): boolean {
  if (containsUrl(content)) return true;
  const words = wordsLower(content);
  return phrases.some((p) => phraseIn(words, p));
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
  const kept = splitWs(noBrackets).filter((t) => !looksLikeUrl(t));
  const joined = kept.join(" ");
  if (joined === collapseWs(s)) return s;
  return trimDangling(joined);
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
  const tokens = featTokens(cs);
  if (tokens.length === 0) return null;
  const [start, , end] = tokens[0];
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
const SMALL_WORDS = [
  "a", "an", "the", "and", "but", "or", "nor", "for", "so", "yet", "as", "at", "by", "in", "of", "on", "to", "up",
  "via", "vs", "vs.", "with", "from", "into", "onto", "per", "feat", "feat.", "ft", "ft.", "featuring", "if", "off",
];
const isSmallWord = (w: string) => SMALL_WORDS.includes(w.toLowerCase());

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

export function titleCase(s: string, keepUpper: string[]): string {
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
    else if (!clauseStart && isSmallWord(text)) converted = text.toLowerCase();
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

const ARTIST_SEPARATORS = [", ", " & ", " / ", " feat. ", " feat ", " ft. ", " ft ", " featuring ", " vs. ", " vs ", " x ", " and "];

function splitArtists(s: string): [string, string][] {
  const cs = chars(s);
  const lower = chars(s.toLowerCase());
  if (lower.length !== cs.length) return [[s, ""]];
  const parts: [string, string][] = [];
  let start = 0;
  let i = 0;
  while (i < cs.length) {
    let hit = 0;
    for (const sep of ARTIST_SEPARATORS) {
      const sc = chars(sep);
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

function applyArtistMap(value: string, map: MapEntry[]): string {
  const whole = lookup(map, value);
  if (whole !== null) return whole;
  const parts = splitArtists(value);
  if (parts.length === 1) return value;
  return parts.map(([name, sep]) => (lookup(map, name) ?? name) + sep).join("");
}

// ---- the pipeline ----
export function cleanFields(cur: Fields, s: CleanupSettings): Fields {
  const w: Fields = { ...cur };
  const get = (f: string) => w[f] ?? "";
  const edit = (f: string, v: string) => {
    if (v !== get(f)) w[f] = collapseWs(v);
  };
  if (s.trim) for (const f of TEXT_FIELDS) w[f] = collapseWs(get(f));
  if (s.strip_promo) for (const f of s.promo_fields) edit(f, stripPromo(get(f), s.promo_phrases));
  if (s.strip_number) edit("title", stripTrackNumber(get("title"), s.number_space_only));
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
  if (s.title_case) for (const f of s.case_fields) edit(f, titleCase(get(f), s.keep_upper));
  if (s.genre_map_on) {
    const to = lookup(s.genre_map, get("genre"));
    if (to !== null) w.genre = to;
  }
  if (s.artist_map_on) for (const f of ["artist", "album_artist"]) edit(f, applyArtistMap(get(f), s.artist_map));
  if (s.trim) for (const f of TEXT_FIELDS) w[f] = collapseWs(get(f));
  return w;
}

export const MAX_TEXT_CHARS = 255;
export const problemOf = (field: string, v: string): string | null => {
  if (chars(v).length > MAX_TEXT_CHARS) return `${field} would be longer than ${MAX_TEXT_CHARS} characters`;
  if (v.includes("\0")) return `${field} would contain a NUL character`;
  return null;
};

