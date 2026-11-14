// The Tracks table's columns, and the (try/catch-guarded) browser storage that remembers which
// are shown, their order and widths, the sort, and two view settings. Pure apart from the
// storage calls, so the merge rules run under plain node (columns.test.mjs).
import type { SortField, SortSpec } from "./trackLogic.ts";
import { loadPref, savePref } from "../../state/prefStore.ts";

export type ColKey =
  | "fav" | "colour" | "title" | "artist" | "album" | "wave" | "bpm" | "key" | "energy" | "autoenergy" | "rating" | "plays" | "time" | "genre" | "tags"
  | "year" | "label" | "remixer" | "composer" | "comments" | "added" | "lastplayed" | "format" | "bitrate" | "size" | "cases" | "caseno";

export type ColDef = {
  key: ColKey;
  label: string;
  width: number;
  min: number;
  num?: boolean;
  sort?: SortField;
  /** Always shown (not in the column chooser). */
  fixed?: boolean;
  defaultOn: boolean;
};

/** Every column in its default order. Plays is the play count and Time the duration. */
export const COLUMNS: ColDef[] = [
  { key: "fav", label: "", width: 34, min: 34, sort: "fav", fixed: true, defaultOn: true },
  // colour, energy and tags are app-side labels (state/labels.ts), not data on the device
  { key: "colour", label: "Colour", width: 58, min: 44, sort: "colour", defaultOn: true },
  { key: "title", label: "Title", width: 190, min: 90, sort: "title", fixed: true, defaultOn: true },
  { key: "artist", label: "Artist", width: 140, min: 70, sort: "artist", defaultOn: true },
  { key: "album", label: "Album", width: 160, min: 70, sort: "album", defaultOn: false },
  { key: "wave", label: "Waveform", width: 100, min: 70, defaultOn: true },
  { key: "bpm", label: "BPM", width: 64, min: 50, num: true, sort: "bpm", defaultOn: true },
  { key: "key", label: "Key", width: 54, min: 46, sort: "key", defaultOn: true },
  { key: "energy", label: "Energy", width: 84, min: 60, sort: "energy", defaultOn: true },
  // read-only estimate from the audio, kept app-side (views/analysis); blank until analysed. It is
  // not the energy label above, which the user sets.
  { key: "autoenergy", label: "Energy (auto)", width: 112, min: 70, sort: "auto_energy", defaultOn: true },
  { key: "rating", label: "Rating", width: 92, min: 90, sort: "rating", defaultOn: true },
  { key: "plays", label: "Plays", width: 48, min: 44, num: true, sort: "times_played", defaultOn: true },
  { key: "time", label: "Time", width: 56, min: 50, num: true, sort: "play_time_secs", defaultOn: true },
  { key: "genre", label: "Genre", width: 100, min: 60, sort: "genre", defaultOn: true },
  { key: "tags", label: "Tags", width: 170, min: 70, sort: "tags", defaultOn: true },
  // columns for what music.db holds beyond the above; hidden until chosen
  { key: "year", label: "Year", width: 64, min: 52, num: true, sort: "year", defaultOn: false },
  { key: "label", label: "Label", width: 130, min: 60, sort: "label", defaultOn: false },
  { key: "remixer", label: "Remixer", width: 130, min: 60, sort: "remixer", defaultOn: false },
  { key: "composer", label: "Composer", width: 130, min: 60, sort: "composer", defaultOn: false },
  { key: "comments", label: "Comments", width: 180, min: 70, sort: "comments", defaultOn: false },
  { key: "added", label: "Date added", width: 92, min: 80, sort: "date_added", defaultOn: false },
  { key: "lastplayed", label: "Last played", width: 92, min: 80, sort: "last_played", defaultOn: false },
  { key: "format", label: "Format", width: 64, min: 50, sort: "format", defaultOn: false },
  { key: "bitrate", label: "Bitrate", width: 78, min: 56, num: true, sort: "bit_rate", defaultOn: false },
  { key: "size", label: "Size", width: 76, min: 56, num: true, sort: "file_size", defaultOn: false },
  // the cases (crates) that hold the track, and its place in the case chosen in the filter bar
  { key: "cases", label: "Cases", width: 170, min: 70, sort: "cases", defaultOn: false },
  { key: "caseno", label: "Case #", width: 62, min: 50, num: true, sort: "case_order", defaultOn: false },
];

/** Columns that only make sense while a case is chosen in the filter bar. */
export const CASE_ONLY: ColKey[] = ["caseno"];

export type ViewPrefs = {
  visible: Record<string, boolean>;
  widths: Record<string, number>;
  /** Column order, every column exactly once; the heart column is always first. */
  order: ColKey[];
  /** The sort, primary key first. */
  sort: SortSpec[];
  /** Restore each library's search, filters and sort when it is opened again. */
  rememberView: boolean;
  /** "Find" in History clears the filters that hide the track instead of only saying so. */
  findClearsFilters: boolean;
  /** Choosing a case in the filter bar sorts by the case's own order (the sort before is put back when the case is cleared). */
  caseOrderOnPick: boolean;
};

const STORAGE_KEY = "pm.tracks.view.v1";
const PINNED_FIRST: ColKey = "fav";
const KEYS = COLUMNS.map((c) => c.key);

export function defaultPrefs(): ViewPrefs {
  return {
    visible: Object.fromEntries(COLUMNS.map((c) => [c.key, c.defaultOn])),
    widths: Object.fromEntries(COLUMNS.map((c) => [c.key, c.width])),
    order: [...KEYS],
    sort: [{ field: "artist", asc: true }],
    rememberView: true,
    findClearsFilters: true,
    caseOrderOnPick: true,
  };
}

const SORT_FIELDS = new Set<string>(COLUMNS.flatMap((c) => (c.sort ? [c.sort] : [])));

/** A sort list read from storage or a saved view: known fields only, each once, at most one per
 *  column; null when nothing usable is left. */
export function cleanSort(raw: unknown): SortSpec[] | null {
  if (!Array.isArray(raw)) return null;
  const seen = new Set<string>();
  const out: SortSpec[] = [];
  for (const k of raw) {
    if (!k || typeof k !== "object") continue;
    const { field, asc } = k as Partial<SortSpec>;
    if (typeof field !== "string" || !SORT_FIELDS.has(field) || seen.has(field) || typeof asc !== "boolean") continue;
    seen.add(field);
    out.push({ field: field as SortField, asc });
  }
  return out.length > 0 ? out : null;
}

/** A saved column order made whole: unknown and repeated keys dropped, columns the saved order
 *  does not know (added by a newer build) placed after the column that precedes them by default,
 *  the heart column first. */
export function cleanOrder(raw: unknown): ColKey[] {
  const known = new Set<string>(KEYS);
  const out: ColKey[] = [];
  if (Array.isArray(raw)) for (const k of raw) if (typeof k === "string" && known.has(k) && !out.includes(k as ColKey)) out.push(k as ColKey);
  KEYS.forEach((k, i) => {
    if (out.includes(k)) return;
    const before = KEYS.slice(0, i).reverse().find((p) => out.includes(p));
    out.splice(before ? out.indexOf(before) + 1 : 0, 0, k);
  });
  return [PINNED_FIRST, ...out.filter((k) => k !== PINNED_FIRST)];
}

/** Saved prefs merged over the defaults, ignoring anything unknown or malformed. */
export function mergePrefs(saved: unknown, base: ViewPrefs = defaultPrefs()): ViewPrefs {
  const prefs: ViewPrefs = structuredClone(base);
  if (!saved || typeof saved !== "object") return prefs;
  const s = saved as Record<string, unknown> & Partial<ViewPrefs>;
  for (const c of COLUMNS) {
    const v = s.visible?.[c.key];
    if (typeof v === "boolean" && !c.fixed) prefs.visible[c.key] = v;
    const w = s.widths?.[c.key];
    if (typeof w === "number" && Number.isFinite(w)) prefs.widths[c.key] = Math.max(c.min, Math.min(800, Math.round(w)));
  }
  prefs.order = cleanOrder(s.order);
  // the single-key shape of earlier builds: sortKey + sortAsc
  const legacy = typeof s.sortKey === "string" ? [{ field: s.sortKey, asc: typeof s.sortAsc === "boolean" ? s.sortAsc : true }] : null;
  prefs.sort = cleanSort(s.sort) ?? cleanSort(legacy) ?? prefs.sort;
  if (typeof s.rememberView === "boolean") prefs.rememberView = s.rememberView;
  if (typeof s.findClearsFilters === "boolean") prefs.findClearsFilters = s.findClearsFilters;
  if (typeof s.caseOrderOnPick === "boolean") prefs.caseOrderOnPick = s.caseOrderOnPick;
  return prefs;
}

/** The columns to draw: in the saved order, hidden ones left out. */
export function shownColumns(prefs: Pick<ViewPrefs, "order" | "visible">, caseChosen = false): ColDef[] {
  const byKey = new Map(COLUMNS.map((c) => [c.key, c] as const));
  return prefs.order.flatMap((k) => {
    const c = byKey.get(k);
    if (c && CASE_ONLY.includes(c.key)) return caseChosen ? [c] : [];
    return c && (c.fixed || prefs.visible[c.key]) ? [c] : [];
  });
}

/** `order` with `key` moved to just before `beforeKey` (the end when null). The heart column
 *  stays first: it cannot be moved and nothing goes before it. */
export function moveColumn(order: readonly ColKey[], key: ColKey, beforeKey: ColKey | null): ColKey[] {
  if (key === PINNED_FIRST || key === beforeKey) return [...order];
  const rest = order.filter((k) => k !== key);
  let at = beforeKey === null ? rest.length : rest.indexOf(beforeKey);
  if (at < 0) at = rest.length;
  at = Math.max(at, rest.indexOf(PINNED_FIRST) + 1);
  rest.splice(at, 0, key);
  return rest;
}

/** Where `key` goes when moved one step left (-1) or right (1) among the `shown` columns: the
 *  `before` argument `moveColumn` takes, or `ok: false` when it cannot move that way. */
export function stepTarget(shown: readonly ColKey[], key: ColKey, step: -1 | 1): { ok: boolean; before: ColKey | null } {
  const i = shown.indexOf(key);
  if (i < 0 || key === PINNED_FIRST) return { ok: false, before: null };
  if (step === -1) {
    const target = shown[i - 1];
    return target === undefined || target === PINNED_FIRST ? { ok: false, before: null } : { ok: true, before: target };
  }
  if (i === shown.length - 1) return { ok: false, before: null };
  return { ok: true, before: shown[i + 2] ?? null };
}

/** The storage key of the Tracks view's layout; the Mix tab's table has its own. */
export const TRACKS_KEY = STORAGE_KEY;
export const MIX_PICKER_KEY = "pm.mixes.picker.v1";

/** Defaults of the Mix tab's table: the same columns as the Tracks view, with Cases shown (the
 *  point of picking from a crate), and a title-first sort that is easy to scan. */
export function mixPickerDefaults(): ViewPrefs {
  const d = defaultPrefs();
  d.visible.cases = true;
  d.widths.wave = 90;
  return d;
}

export function loadPrefs(key: string = STORAGE_KEY, base: () => ViewPrefs = defaultPrefs): ViewPrefs {
  return loadPref(key, (raw) => mergePrefs(raw, base()), () => base());
}

export function savePrefs(prefs: ViewPrefs, key: string = STORAGE_KEY) {
  savePref(key, prefs); // a failed save: the table still works, it just forgets
}
