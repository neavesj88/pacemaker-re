// Pure logic of the printable setlist: rows from a case or a mix, time and field formatting, the
// user's print options, and the one function that builds the HTML page. The page is built here
// and nowhere else, so the preview, the print and the saved file are the same text.
//
// No store or component imports (types only), so it runs under plain node:
// `node --experimental-strip-types --test src/lib/views/setlist/setlistLogic.test.mjs`.
import type { MixDoc } from "$lib/api/mixes";
import type { Track } from "$lib/api/types";

// ---- options ------------------------------------------------------------------------------

export type Column = "no" | "time" | "length" | "artist" | "title" | "bpm" | "key" | "energy" | "note";

/** Columns in print order, with the label the page and the options use. */
export const COLUMNS: { id: Column; label: string }[] = [
  { id: "no", label: "No" },
  { id: "time", label: "Time" },
  { id: "length", label: "Length" },
  { id: "artist", label: "Artist" },
  { id: "title", label: "Title" },
  { id: "bpm", label: "BPM" },
  { id: "key", label: "Key" },
  { id: "energy", label: "Energy" },
  { id: "note", label: "Notes" },
];

export type TimeMode = "elapsed" | "clock";
export type KeyNotation = "stored" | "camelot" | "name";
export type Paper = "auto" | "a4" | "letter";
export type TextSize = "normal" | "large" | "xlarge";

/** Everything about how the page looks. Saved as a user setting; none of it is a fixed rule. */
export type Options = {
  columns: Record<Column, boolean>;
  /** Leave out a column that has nothing in it for any row (notes nobody wrote, no energy ratings). */
  hideEmpty: boolean;
  timeMode: TimeMode;
  keyNotation: KeyNotation;
  paper: Paper;
  landscape: boolean;
  size: TextSize;
  /** Mix only: list clips that are muted in the mix (default: leave them out). */
  includeMuted: boolean;
  showTotals: boolean;
  zebra: boolean;
};

export const DEFAULT_OPTIONS: Options = {
  columns: { no: true, time: true, length: false, artist: true, title: true, bpm: true, key: true, energy: true, note: true },
  hideEmpty: true,
  timeMode: "elapsed",
  keyNotation: "stored",
  paper: "auto",
  landscape: false,
  size: "normal",
  includeMuted: false,
  showTotals: true,
  zebra: true,
};

const oneOf = <T extends string>(v: unknown, allowed: readonly T[], fallback: T): T => (allowed.includes(v as T) ? (v as T) : fallback);
const flag = (v: unknown, fallback: boolean) => (typeof v === "boolean" ? v : fallback);

/** Options read back from storage: anything missing or wrong falls back to the default. The
 * title column cannot be turned off (a setlist without titles says nothing). */
export function cleanOptions(raw: unknown): Options {
  const r = (raw && typeof raw === "object" ? raw : {}) as Record<string, unknown>;
  const c = (r.columns && typeof r.columns === "object" ? r.columns : {}) as Record<string, unknown>;
  const columns = { ...DEFAULT_OPTIONS.columns };
  for (const { id } of COLUMNS) columns[id] = flag(c[id], DEFAULT_OPTIONS.columns[id]);
  columns.title = true;
  return {
    columns,
    hideEmpty: flag(r.hideEmpty, DEFAULT_OPTIONS.hideEmpty),
    timeMode: oneOf(r.timeMode, ["elapsed", "clock"], DEFAULT_OPTIONS.timeMode),
    keyNotation: oneOf(r.keyNotation, ["stored", "camelot", "name"], DEFAULT_OPTIONS.keyNotation),
    paper: oneOf(r.paper, ["auto", "a4", "letter"], DEFAULT_OPTIONS.paper),
    landscape: flag(r.landscape, DEFAULT_OPTIONS.landscape),
    size: oneOf(r.size, ["normal", "large", "xlarge"], DEFAULT_OPTIONS.size),
    includeMuted: flag(r.includeMuted, DEFAULT_OPTIONS.includeMuted),
    showTotals: flag(r.showTotals, DEFAULT_OPTIONS.showTotals),
    zebra: flag(r.zebra, DEFAULT_OPTIONS.zebra),
  };
}

// ---- the sheet: what the user types -------------------------------------------------------

/** What the user typed for one track. Energy is 1..10, 0 = not set. */
export type SheetEntry = { track_id: number; note: string; energy: number };

/** Header text and per-track entries of one setlist (the Rust `Sheet`, field names the same). */
export type Sheet = {
  title: string;
  event: string;
  venue: string;
  date: string;
  dj: string;
  footer: string;
  /** "HH:MM" or empty. */
  start_clock: string;
  entries: SheetEntry[];
};

export const EMPTY_SHEET: Sheet = { title: "", event: "", venue: "", date: "", dj: "", footer: "", start_clock: "", entries: [] };

export const MAX_ENERGY = 10;

/** A copy of the sheet with one track's note and / or energy changed. An entry with nothing in
 * it is removed. */
export function withEntry(sheet: Sheet, trackId: number, patch: Partial<Pick<SheetEntry, "note" | "energy">>): Sheet {
  const cur = sheet.entries.find((e) => e.track_id === trackId) ?? { track_id: trackId, note: "", energy: 0 };
  const next = { ...cur, ...patch };
  const rest = sheet.entries.filter((e) => e.track_id !== trackId);
  return { ...sheet, entries: next.note === "" && next.energy === 0 ? rest : [...rest, next] };
}

export const entryOf = (sheet: Sheet, trackId: number): SheetEntry =>
  sheet.entries.find((e) => e.track_id === trackId) ?? { track_id: trackId, note: "", energy: 0 };

/** "HH:MM" on a 24-hour clock (one or two hour digits), or null. */
export function parseClock(text: string): { h: number; m: number } | null {
  const m = /^(\d{1,2}):(\d{2})$/.exec(text.trim());
  if (!m) return null;
  const h = Number(m[1]);
  const min = Number(m[2]);
  return h <= 23 && min <= 59 ? { h, m: min } : null;
}

// ---- rows ---------------------------------------------------------------------------------

/** One line of the setlist before the user's notes are added. */
export type SetRow = {
  trackId: number;
  /** The track is not in the library any more (a case can still list its id). */
  missing: boolean;
  /** Mix only: the clip is muted in the mix. */
  muted: boolean;
  /** Seconds from the start of the set, or null when unknown. */
  startSec: number | null;
  lengthSec: number | null;
  artist: string;
  title: string;
  /** 0 = unknown / not analysed. */
  bpm: number;
  key: string;
  /** Energy 1 to 10 of the track in the library under the "Energy source" setting (your rating, the
   *  audio estimate, or yours else the estimate); null when it has none. An energy typed for this
   *  setlist in the sheet comes first (see `shownEnergy`). */
  energy: number | null;
};

/** Rows plus the things to tell the user about them. */
export type Built = { rows: SetRow[]; totalSec: number; notes: string[] };

type TrackLike = Pick<Track, "title" | "artist" | "bpm" | "key" | "play_time_secs">;

/** The library energy of a track (see `SetRow.energy`). */
export type EnergyOf = (trackId: number) => number | null;

/** The energy printed for a row: the one typed for this setlist, else the one from the library, else 0 (none). */
export function shownEnergy(row: Pick<SetRow, "energy">, entry: Pick<SheetEntry, "energy">): number {
  return entry.energy > 0 ? entry.energy : (row.energy ?? 0);
}

const plural = (n: number, one: string, many = `${one}s`) => `${n} ${n === 1 ? one : many}`;

/** The rows of a case, in case order. A track listed twice in the case is listed once (the case
 * view does the same). Start times are the running total of the track lengths. `noBpm` holds ids
 * whose tempo is not trusted (needs analysis): they print without one, as in the case view. */
export function rowsFromCase(ids: readonly number[], byId: ReadonlyMap<number, TrackLike>, noBpm: ReadonlySet<number> = new Set(), energyOf: EnergyOf = () => null): Built {
  const rows: SetRow[] = [];
  let at = 0;
  let missing = 0;
  for (const id of new Set(ids)) {
    const t = byId.get(id);
    const len = t ? Math.max(0, t.play_time_secs || 0) : 0;
    if (!t) missing++;
    rows.push({
      trackId: id,
      missing: !t,
      muted: false,
      startSec: at,
      lengthSec: t ? len : null,
      artist: t?.artist ?? "",
      title: t?.title ?? "",
      bpm: t && !noBpm.has(id) ? t.bpm : 0,
      key: t?.key ?? "",
      energy: t ? energyOf(id) : null,
    });
    at += len;
  }
  const notes = missing > 0 ? [`${plural(missing, "track")} in this case ${missing === 1 ? "is" : "are"} no longer in the library and print as a gap.`] : [];
  return { rows, totalSec: at, notes };
}

/** The rows of a mix, in the order they start. One row per clip; a track used twice appears
 * twice. Start times are mix time from the first audible frame (where an exported mix starts),
 * and length is how long the clip is audible, so overlapping clips overlap here too. Muted
 * clips are left out unless `includeMuted`. */
export function rowsFromMix(doc: MixDoc, byId: ReadonlyMap<number, TrackLike>, includeMuted: boolean, noBpm: ReadonlySet<number> = new Set(), energyOf: EnergyOf = () => null): Built {
  const indexed = doc.clips.map((clip, i) => ({ clip, i }));
  const kept = indexed.filter(({ clip }) => includeMuted || !clip.clipBypassed);
  const leftOut = indexed.length - kept.length;
  kept.sort((a, b) => a.clip.visibleStartSec - b.clip.visibleStartSec || a.clip.channel - b.clip.channel || a.i - b.i);
  const first = kept.length > 0 ? kept[0].clip.visibleStartSec : 0;
  let end = first;
  let missing = 0;
  const rows: SetRow[] = kept.map(({ clip }) => {
    const info = clip.track;
    const lib = byId.get(clip.trackId);
    const gone = clip.trackId < 0 || (!info && !lib);
    if (gone) missing++;
    const bpm = info?.bpm ?? lib?.bpm ?? clip.bpm ?? 0;
    end = Math.max(end, clip.visibleEndSec);
    return {
      trackId: clip.trackId,
      missing: gone,
      muted: clip.clipBypassed,
      startSec: Math.max(0, clip.visibleStartSec - first),
      lengthSec: Math.max(0, clip.visibleEndSec - clip.visibleStartSec),
      artist: info?.artist ?? lib?.artist ?? "",
      title: info?.title ?? lib?.title ?? "",
      bpm: noBpm.has(clip.trackId) ? 0 : bpm,
      key: info?.key ?? lib?.key ?? "",
      energy: gone ? null : energyOf(clip.trackId),
    };
  });
  const notes: string[] = [];
  if (leftOut > 0) notes.push(`${plural(leftOut, "muted clip")} left out. Turn on "Include muted clips" to list ${leftOut === 1 ? "it" : "them"}.`);
  if (missing > 0) notes.push(`${plural(missing, "clip")} use${missing === 1 ? "s" : ""} a track that is no longer in the library and print${missing === 1 ? "s" : ""} as a gap.`);
  return { rows, totalSec: Math.max(0, end - first), notes };
}

// ---- formatting ---------------------------------------------------------------------------

/** "3:05", or "1:02:03" from an hour up. Seconds are cut, not rounded, so a time never claims
 * more than has elapsed. */
export function fmtElapsed(sec: number): string {
  const s = Math.max(0, Math.floor(Number.isFinite(sec) ? sec : 0));
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const r = String(s % 60).padStart(2, "0");
  return h > 0 ? `${h}:${String(m).padStart(2, "0")}:${r}` : `${m}:${r}`;
}

/** The clock time `offsetSec` after `start` ("HH:MM"), as "HH:MM", wrapping past midnight;
 * null when `start` is not a valid time. Seconds are cut. */
export function fmtClock(start: string, offsetSec: number): string | null {
  const c = parseClock(start);
  if (!c) return null;
  const total = c.h * 60 + c.m + Math.floor(Math.max(0, offsetSec) / 60);
  const day = ((total % 1440) + 1440) % 1440;
  return `${String(Math.floor(day / 60)).padStart(2, "0")}:${String(day % 60).padStart(2, "0")}`;
}

/** 128 -> "128", 127.54 -> "127.5", 0 or NaN -> "". */
export function fmtBpm(bpm: number): string {
  return Number.isFinite(bpm) && bpm > 0 ? String(Number(bpm.toFixed(1))) : "";
}

/** How a key is read: a Camelot code and a spoken name, or null when it is not recognised. */
export type KeyParser = (key: string) => { code: string; name: string } | null;

/** A key as printed. "stored" is the text on the device as it is; the other two convert when the
 * key is recognised and otherwise keep the stored text rather than print nothing. */
export function fmtKey(key: string, notation: KeyNotation, parse: KeyParser): string {
  const k = key.trim();
  if (!k || notation === "stored") return k;
  const p = parse(k);
  return p ? (notation === "camelot" ? p.code : p.name) : k;
}

/** A name that is safe as a file name on Windows, macOS and Linux. */
export function fileNameFor(title: string): string {
  const base = title
    .replace(/[\\/:*?"<>|\u0000-\u001f]/g, " ")
    .replace(/\s+/g, " ")
    .trim()
    .replace(/[. ]+$/, "")
    .slice(0, 80)
    .trim();
  return `${base || "Setlist"} setlist.html`;
}

// ---- the page -----------------------------------------------------------------------------

/** Escapes text for HTML (element content and attribute values). */
export function esc(s: string): string {
  return s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;").replace(/'/g, "&#39;");
}

export type PageInput = {
  /** Name of the case or mix: the heading unless the sheet has its own title. */
  name: string;
  kind: "case" | "mix";
  rows: readonly SetRow[];
  totalSec: number;
  sheet: Sheet;
  options: Options;
  parseKey: KeyParser;
  /** A line shown on screen only (never printed) telling a reader of the saved file how to print. */
  screenHint: boolean;
};

/** The columns the page shows, in print order. Title always; the others when switched on and,
 * with `hideEmpty`, only if some row has something in that column. */
export function visibleColumns(input: Pick<PageInput, "rows" | "sheet" | "options" | "parseKey">): Column[] {
  const { rows, sheet, options } = input;
  const has: Record<Column, boolean> = {
    no: true,
    time: true,
    length: rows.some((r) => r.lengthSec !== null),
    artist: rows.some((r) => r.artist.trim() !== ""),
    title: true,
    bpm: rows.some((r) => fmtBpm(r.bpm) !== ""),
    key: rows.some((r) => r.key.trim() !== ""),
    energy: rows.some((r) => shownEnergy(r, entryOf(sheet, r.trackId)) > 0),
    note: rows.some((r) => entryOf(sheet, r.trackId).note.trim() !== ""),
  };
  return COLUMNS.map((c) => c.id).filter((id) => id === "title" || (options.columns[id] && (!options.hideEmpty || has[id])));
}

const FONT_PT: Record<TextSize, number> = { normal: 11, large: 14, xlarge: 18 };
const PAGE_WIDTH_MM: Record<Paper, [number, number]> = { auto: [210, 297], a4: [210, 297], letter: [216, 279] };

function css(o: Options): string {
  const [w, h] = PAGE_WIDTH_MM[o.paper];
  const width = o.landscape ? h : w;
  const size = o.paper === "auto" ? (o.landscape ? "landscape" : "portrait") : `${o.paper === "a4" ? "A4" : "letter"} ${o.landscape ? "landscape" : "portrait"}`;
  return `
:root { --ink: #111; --soft: #555; --rule: #b5b5b5; --band: #f0f0f0; --desk: #e4e4e4; --paper: #fff; }
@page { size: ${size}; margin: 14mm; }
* { box-sizing: border-box; }
html { background: var(--desk); }
body { margin: 0; color: var(--ink); font: ${FONT_PT[o.size]}pt/1.35 system-ui, "Segoe UI", Roboto, Helvetica, Arial, sans-serif; }
.page { max-width: ${width}mm; margin: 16px auto; padding: 14mm; background: var(--paper); box-shadow: 0 1px 6px rgba(0, 0, 0, 0.25); }
h1 { margin: 0 0 0.2em; font-size: 1.7em; line-height: 1.15; }
.meta { display: flex; flex-wrap: wrap; gap: 0.1em 1.6em; margin: 0 0 0.9em; color: var(--soft); font-size: 0.95em; }
.meta b { color: var(--ink); font-weight: 600; }
table { width: 100%; border-collapse: collapse; }
thead { display: table-header-group; }
th { padding: 0.25em 0.5em; text-align: left; font-size: 0.78em; font-weight: 700; letter-spacing: 0.05em; text-transform: uppercase; color: var(--soft); border-bottom: 2px solid var(--ink); }
td { padding: 0.28em 0.5em; vertical-align: top; border-bottom: 1px solid var(--rule); }
tr { break-inside: avoid; page-break-inside: avoid; }
${o.zebra ? "tbody tr:nth-child(even) td { background: var(--band); -webkit-print-color-adjust: exact; print-color-adjust: exact; }" : ""}
.num { text-align: right; font-variant-numeric: tabular-nums; white-space: nowrap; }
.nowrap { white-space: nowrap; }
.c-title { font-weight: 600; }
.c-artist { min-width: 9em; }
.c-note { min-width: 28%; }
.gap td { color: var(--soft); font-style: italic; }
.tag { margin-left: 0.5em; padding: 0 0.4em; border: 1px solid var(--rule); border-radius: 3px; font-size: 0.75em; font-style: normal; font-weight: 400; color: var(--soft); }
.totals { margin: 0.7em 0 0; color: var(--soft); font-size: 0.95em; }
.footer { margin: 0.9em 0 0; white-space: pre-wrap; }
.hint { max-width: ${width}mm; margin: 12px auto 0; padding: 0 14mm; color: #444; font: 10pt system-ui, sans-serif; }
@media print {
  html { background: none; }
  .page { max-width: none; margin: 0; padding: 0; box-shadow: none; }
  .hint { display: none; }
}`;
}

/** The complete setlist page as one HTML document, with its styles inline and no scripts, so a
 * saved copy opens anywhere and prints the same as the preview. All text from the library or the
 * user is escaped. */
export function buildDocument(input: PageInput): string {
  const { rows, sheet, options } = input;
  const cols = visibleColumns(input);
  const clock = options.timeMode === "clock" && parseClock(sheet.start_clock) !== null;
  const label = (id: Column) => (id === "time" ? (clock ? "Time" : "Start") : (COLUMNS.find((c) => c.id === id)?.label ?? id));
  const heading = sheet.title.trim() || input.name.trim() || (input.kind === "mix" ? "Mix" : "Setlist");

  const meta = [
    ["Event", sheet.event],
    ["Venue", sheet.venue],
    ["Date", sheet.date],
    ["DJ", sheet.dj],
  ]
    .filter(([, v]) => v.trim() !== "")
    .map(([k, v]) => `<span><b>${k}</b> ${esc(v.trim())}</span>`)
    .join("");

  const head = cols.map((id) => `<th class="${cellClass(id)}">${esc(label(id))}</th>`).join("");

  const body = rows
    .map((r, i) => {
      const e = entryOf(sheet, r.trackId);
      const cell = (id: Column): string => {
        switch (id) {
          case "no":
            return String(i + 1);
          case "time":
            if (r.startSec === null) return "";
            return clock ? (fmtClock(sheet.start_clock, r.startSec) ?? "") : fmtElapsed(r.startSec);
          case "length":
            return r.lengthSec === null ? "" : fmtElapsed(r.lengthSec);
          case "artist":
            return esc(r.artist);
          case "title":
            return r.missing ? "Track no longer in the library" : esc(r.title) + (r.muted ? '<span class="tag">muted</span>' : "");
          case "bpm":
            return fmtBpm(r.bpm);
          case "key":
            return esc(fmtKey(r.key, options.keyNotation, input.parseKey));
          case "energy":
            return shownEnergy(r, e) > 0 ? String(shownEnergy(r, e)) : "";
          case "note":
            return esc(e.note);
        }
      };
      return `<tr${r.missing ? ' class="gap"' : ""}>${cols.map((id) => `<td class="${cellClass(id)}">${cell(id)}</td>`).join("")}</tr>`;
    })
    .join("\n");

  const totals = options.showTotals ? `<p class="totals">${rows.length} ${rows.length === 1 ? "track" : "tracks"} · ${esc(fmtElapsed(input.totalSec))}</p>` : "";
  const footer = sheet.footer.trim() ? `<p class="footer">${esc(sheet.footer.trim())}</p>` : "";
  const hint = input.screenHint ? '<p class="hint">To print this page, use your browser\'s Print command (Ctrl+P). To make a PDF, choose Save as PDF as the printer.</p>' : "";
  const empty = rows.length === 0 ? '<p class="totals">This setlist has no tracks.</p>' : "";

  return `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>${esc(heading)}</title>
<style>${css(options)}
</style>
</head>
<body>
<div class="page">
<h1>${esc(heading)}</h1>
${meta ? `<div class="meta">${meta}</div>` : ""}
<table>
<thead><tr>${head}</tr></thead>
<tbody>
${body}
</tbody>
</table>
${empty}${totals}
${footer}
</div>
${hint}
</body>
</html>
`;
}

function cellClass(id: Column): string {
  switch (id) {
    case "no":
    case "time":
    case "length":
    case "bpm":
      return `c-${id} num`;
    case "key":
    case "energy":
      return `c-${id} nowrap`;
    default:
      return `c-${id}`;
  }
}
