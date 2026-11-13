// Track matcher: turns pasted or loaded request-list text into request items. Pure (no DOM, no
// Tauri), tested by parseList.test.mjs.
//
// Understood input, all offline:
//   - plain lines, "Artist - Title" (hyphen, en dash, em dash, " -- " or a tab as the separator),
//     with an optional list number, bullet or timestamp in front; "Title - Artist" with the
//     `lineOrder` option; a line without a separator is a title only;
//   - CSV / TSV / semicolon files with a header row that names a title column (Title, Track Name,
//     Song, Name, ...) and optionally an artist column (Artist, Artist Name(s), ...) and a mix or
//     version column. Quoted fields, doubled quotes, line breaks inside quotes and a BOM are handled;
//   - M3U / M3U8 playlists (#EXTINF display text, else the file name of the entry).
// A line that is only a link cannot be resolved without the network and is listed as skipped.

export type LineOrder = "artist-title" | "title-artist";

export type RequestItem = {
  /** Position in `items`; the key everything else (matches, choices) is stored under. */
  id: number;
  /** 1-based line (or CSV row) in the source text where the item first appeared. */
  line: number;
  /** The source line as written (for CSV, the title and artist cells joined). */
  raw: string;
  artist: string;
  title: string;
  /** How many further lines in the list asked for the same artist and title (merged into this one). */
  repeats: number;
};

export type SkippedLine = { line: number; raw: string; reason: string };

export type ParsedList = {
  format: "lines" | "csv" | "m3u";
  items: RequestItem[];
  skipped: SkippedLine[];
  /** Lines merged into an earlier identical request. */
  merged: number;
  /** For CSV: the header names used. */
  columns: { title: string; artist: string | null; mix: string | null } | null;
  /** Set (and `items` empty) when the list cannot be used at all. */
  error: string | null;
};

/** More requests than this are refused rather than silently cut. */
export const MAX_ITEMS = 10_000;
const MAX_LINE_CHARS = 500;

const TITLE_HEADERS = ["track name", "track title", "song title", "song name", "title", "track", "song", "name"];
const ARTIST_HEADERS = ["artist name(s)", "artist names", "artist name", "artist(s)", "artists", "artist", "performer", "performers"];
const MIX_HEADERS = ["mix name", "mix version", "mix", "version"];

// the key two requests are the same under: letters and digits only, case and accents folded
const key = (s: string) =>
  s.normalize("NFKD").replace(/[̀-ͯ]/g, "").toLowerCase().replace(/[^\p{L}\p{N}]+/gu, "");

const unquote = (s: string) => s.trim().replace(/^["“”„'‘’](.*)["“”'‘’]$/s, "$1").trim();

const URL_ONLY = /^(?:https?:\/\/|spotify:|www\.)\S+$/i;
const SEPARATOR = /\s+[-–—―]\s+|\s--\s|\t/;
const LEADER = /^\s*(?:\(?\d{1,4}[.)]|[-*•▪●]|\d{1,2}:\d{2}(?::\d{2})?)\s+/;

/** Splits "Artist - Title" at the first separator. No separator: a title only. */
export function splitArtistTitle(text: string, order: LineOrder = "artist-title"): { artist: string; title: string } {
  const m = SEPARATOR.exec(text);
  if (!m) return { artist: "", title: unquote(text) };
  const left = unquote(text.slice(0, m.index));
  const right = unquote(text.slice(m.index + m[0].length));
  if (!left || !right) return { artist: "", title: left || right };
  return order === "artist-title" ? { artist: left, title: right } : { artist: right, title: left };
}

/** RFC 4180 style records: quoted fields, "" for a quote, line breaks inside quotes. */
export function parseCsv(text: string, delim: string): string[][] {
  const rows: string[][] = [];
  let row: string[] = [];
  let cell = "";
  let quoted = false;
  let any = false;
  for (let i = 0; i < text.length; i++) {
    const c = text[i];
    if (quoted) {
      if (c === '"') {
        if (text[i + 1] === '"') {
          cell += '"';
          i++;
        } else quoted = false;
      } else cell += c;
    } else if (c === '"' && cell === "") {
      quoted = true;
      any = true;
    } else if (c === delim) {
      row.push(cell);
      cell = "";
      any = true;
    } else if (c === "\n" || c === "\r") {
      if (c === "\r" && text[i + 1] === "\n") i++;
      if (any || cell !== "") {
        row.push(cell);
        rows.push(row);
      }
      row = [];
      cell = "";
      any = false;
    } else {
      cell += c;
      any = true;
    }
  }
  if (any || cell !== "") {
    row.push(cell);
    rows.push(row);
  }
  return rows;
}

function findColumn(headers: string[], names: string[]): number {
  for (const n of names) {
    const i = headers.indexOf(n);
    if (i >= 0) return i;
  }
  return -1;
}

type Header = { delim: string; title: number; artist: number; mix: number; names: string[] };

/** A CSV / TSV header: the delimiter and the columns, or null when the first line is not one. */
function detectHeader(firstLine: string): Header | null {
  let best: Header | null = null;
  for (const delim of [",", ";", "\t"]) {
    if (!firstLine.includes(delim) && delim !== ",") continue;
    const names = (parseCsv(firstLine, delim)[0] ?? []).map((h) => h.replace(/^﻿/, "").trim().toLowerCase());
    const title = findColumn(names, TITLE_HEADERS);
    if (title < 0) continue;
    const h: Header = { delim, title, artist: findColumn(names, ARTIST_HEADERS), mix: findColumn(names, MIX_HEADERS), names };
    if (!best || names.length > best.names.length) best = h;
  }
  return best;
}

function add(out: ParsedList, seen: Map<string, RequestItem>, line: number, raw: string, artist: string, title: string) {
  artist = artist.trim();
  title = title.trim();
  if (!title) {
    out.skipped.push({ line, raw, reason: "no title" });
    return;
  }
  const k = key(artist) + "|" + key(title);
  const first = seen.get(k);
  if (first) {
    first.repeats++;
    out.merged++;
    return;
  }
  const item: RequestItem = { id: out.items.length, line, raw, artist, title, repeats: 0 };
  seen.set(k, item);
  out.items.push(item);
}

function parseCsvList(text: string, header: Header, out: ParsedList) {
  const rows = parseCsv(text, header.delim);
  const seen = new Map<string, RequestItem>();
  out.format = "csv";
  out.columns = {
    title: header.names[header.title],
    artist: header.artist >= 0 ? header.names[header.artist] : null,
    mix: header.mix >= 0 ? header.names[header.mix] : null,
  };
  for (let r = 1; r < rows.length; r++) {
    const cells = rows[r];
    if (cells.every((c) => !c.trim())) continue;
    let title = (cells[header.title] ?? "").trim();
    const artist = header.artist >= 0 ? (cells[header.artist] ?? "").trim() : "";
    const mix = header.mix >= 0 ? (cells[header.mix] ?? "").trim() : "";
    // the mix column is appended as "(mix)" unless the title already says it
    if (mix && title && !title.toLowerCase().includes(mix.toLowerCase())) title += ` (${mix})`;
    add(out, seen, r + 1, [artist, title].filter(Boolean).join(" - "), artist, title);
    if (out.items.length > MAX_ITEMS) return;
  }
}

function parseM3u(lines: string[], order: LineOrder, out: ParsedList) {
  const seen = new Map<string, RequestItem>();
  out.format = "m3u";
  let pending: { line: number; text: string } | null = null;
  lines.forEach((l, i) => {
    const t = l.trim();
    if (!t) return;
    const info = /^#EXTINF:[^,]*,(.*)$/i.exec(t);
    if (info) {
      pending = { line: i + 1, text: info[1].trim() };
      return;
    }
    if (t.startsWith("#")) return;
    // an entry: the EXTINF text before it, else its file name without the extension
    if (pending && pending.text) {
      const { artist, title } = splitArtistTitle(pending.text, order);
      add(out, seen, pending.line, pending.text, artist, title);
    } else if (URL_ONLY.test(t)) {
      out.skipped.push({ line: i + 1, raw: t, reason: "a link: titles are not available offline" });
    } else {
      const stem = (t.split(/[\\/]/).pop() ?? t).replace(/\.[A-Za-z0-9]{2,5}$/, "");
      const { artist, title } = splitArtistTitle(stem, order);
      add(out, seen, i + 1, stem, artist, title);
    }
    pending = null;
  });
}

function parseLines(lines: string[], order: LineOrder, out: ParsedList) {
  const seen = new Map<string, RequestItem>();
  out.format = "lines";
  lines.forEach((l, i) => {
    if (out.items.length > MAX_ITEMS) return;
    const t = l.trim();
    if (!t) return;
    if (t.startsWith("#") || t.startsWith("//")) {
      out.skipped.push({ line: i + 1, raw: t.slice(0, 80), reason: "comment line" });
      return;
    }
    if (t.length > MAX_LINE_CHARS) {
      out.skipped.push({ line: i + 1, raw: t.slice(0, 80) + "…", reason: `longer than ${MAX_LINE_CHARS} characters` });
      return;
    }
    if (URL_ONLY.test(t)) {
      out.skipped.push({ line: i + 1, raw: t, reason: "a link: titles are not available offline" });
      return;
    }
    const body = t.replace(LEADER, "").replace(/\s+(?:https?:\/\/|spotify:)\S+$/i, "");
    const { artist, title } = splitArtistTitle(body, order);
    add(out, seen, i + 1, t, artist, title);
  });
}

/** Parses a request list; the format is detected from the text. */
export function parseRequestList(text: string, options: { lineOrder?: LineOrder } = {}): ParsedList {
  const order = options.lineOrder ?? "artist-title";
  const out: ParsedList = { format: "lines", items: [], skipped: [], merged: 0, columns: null, error: null };
  const clean = text.replace(/^﻿/, "");
  const lines = clean.split(/\r\n|\r|\n/);
  const first = lines.find((l) => l.trim()) ?? "";
  if (!first) return out;

  if (/^#EXTM3U/i.test(first.trim()) || lines.some((l) => /^#EXTINF:/i.test(l.trim()))) parseM3u(lines, order, out);
  else {
    const header = detectHeader(first);
    if (header) {
      // the header row must be row 1 of the parse: drop any blank lines in front of it
      parseCsvList(clean.slice(clean.indexOf(first)), header, out);
    } else parseLines(lines, order, out);
  }
  if (out.items.length > MAX_ITEMS) {
    return { ...out, items: [], error: `The list has more than ${MAX_ITEMS.toLocaleString("en")} requests; split it into smaller lists.` };
  }
  return out;
}
