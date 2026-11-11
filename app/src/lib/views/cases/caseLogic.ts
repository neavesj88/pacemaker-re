// Pure logic of the Cases views: the case table's columns and saved view, sorting and searching
// its rows, default names, and the sentences the merge / copy / move dialogs show. No store or
// component imports (types only) so `caseLogic.test.mjs` can run it with plain node.
import type { MergePlan, TransferPlan } from "$lib/api/cases";
import type { Track } from "$lib/api/types";
import type { SortField } from "../tracks/trackLogic";

// ---- the case table's columns -------------------------------------------------------------

/** The Tracks view's columns (same names, same sort fields), so a case can be read and sorted
 *  like the library. `#` is the case's own order; `fav` and `title` are always shown. */
export type CaseColKey = "fav" | "title" | "artist" | "album" | "wave" | "bpm" | "key" | "rating" | "plays" | "time" | "genre";

export type CaseCol = { key: CaseColKey; label: string; num?: boolean; sort: SortField | null; fixed?: boolean; defaultOn: boolean };

export const CASE_COLUMNS: CaseCol[] = [
  { key: "fav", label: "", sort: "fav", fixed: true, defaultOn: true },
  { key: "title", label: "Title", sort: "title", fixed: true, defaultOn: true },
  { key: "artist", label: "Artist", sort: "artist", defaultOn: true },
  { key: "album", label: "Album", sort: "album", defaultOn: false },
  { key: "genre", label: "Genre", sort: "genre", defaultOn: false },
  { key: "wave", label: "Waveform", sort: null, defaultOn: false },
  { key: "bpm", label: "BPM", num: true, sort: "bpm", defaultOn: true },
  { key: "key", label: "Key", sort: "key", defaultOn: true },
  { key: "rating", label: "Rating", sort: "rating", defaultOn: true },
  { key: "plays", label: "Plays", num: true, sort: "times_played", defaultOn: false },
  { key: "time", label: "Time", num: true, sort: "play_time_secs", defaultOn: true },
];

/** "order" is the case's own order (what the Pacemaker plays); anything else is a view-only sort. */
export type CaseSortKey = SortField | "order";

export type CasePrefs = {
  visible: Record<string, boolean>;
  sortKey: CaseSortKey;
  sortAsc: boolean;
  /** Merge dialog: delete the source cases after merging. Off unless the user turns it on. */
  mergeDeletesSources: boolean;
};

export function defaultCasePrefs(): CasePrefs {
  return { visible: Object.fromEntries(CASE_COLUMNS.map((c) => [c.key, c.defaultOn])), sortKey: "order", sortAsc: true, mergeDeletesSources: false };
}

const SORT_FIELDS = new Set<string>(CASE_COLUMNS.flatMap((c) => (c.sort ? [c.sort] : [])));

/** Saved prefs merged over the defaults, ignoring anything unknown or malformed. */
export function mergeCasePrefs(saved: unknown): CasePrefs {
  const prefs = defaultCasePrefs();
  if (!saved || typeof saved !== "object") return prefs;
  const s = saved as Partial<CasePrefs>;
  for (const c of CASE_COLUMNS) {
    const v = s.visible?.[c.key];
    if (typeof v === "boolean" && !c.fixed) prefs.visible[c.key] = v;
  }
  if (s.sortKey === "order" || (typeof s.sortKey === "string" && SORT_FIELDS.has(s.sortKey))) prefs.sortKey = s.sortKey;
  if (typeof s.sortAsc === "boolean") prefs.sortAsc = s.sortAsc;
  if (typeof s.mergeDeletesSources === "boolean") prefs.mergeDeletesSources = s.mergeDeletesSources;
  return prefs;
}

// ---- rows ---------------------------------------------------------------------------------

/** One line of a case: the id as stored, and the track if it is still in the library. */
export type Row = { id: number; t: Track | undefined };

export type TrackSorter = (rows: Track[], field: SortField, asc: boolean, isFavourite: (id: number) => boolean) => Track[];

/** `rows` in view order. "order" keeps the case's own order (reversed when descending); any
 *  other key sorts the tracks with `sorter` (the Tracks view's `sortTracks`, so the two tables
 *  agree) and leaves rows whose track is gone at the end, in their own order. */
export function sortRows(rows: Row[], key: CaseSortKey, asc: boolean, sorter: TrackSorter, isFavourite: (id: number) => boolean): Row[] {
  if (key === "order") return asc ? rows : [...rows].reverse();
  const present = rows.filter((r) => r.t);
  const byId = new Map(present.map((r) => [r.id, r]));
  const sorted = sorter(present.map((r) => r.t!), key, asc, isFavourite).map((t) => byId.get(t.track_id)!);
  return [...sorted, ...rows.filter((r) => !r.t)];
}

/** Rows whose track matches the header search box (title, artist, album, genre or key). */
export function searchRows(rows: Row[], query: string): Row[] {
  const q = query.trim().toLowerCase();
  if (!q) return rows;
  return rows.filter((r) => r.t && [r.t.title, r.t.artist, r.t.album, r.t.genre, r.t.key].some((f) => f.toLowerCase().includes(q)));
}

/** "Title by Artist", or a note for a track that is no longer in the library. */
export const trackLabel = (id: number, t: Track | undefined) =>
  t ? `${t.title}${t.artist ? ` by ${t.artist}` : ""}` : `track #${id} (no longer in the library)`;

// ---- names and sentences --------------------------------------------------------------------

export const plural = (n: number, word: string) => `${n} ${word}${n === 1 ? "" : "s"}`;

/** "Name copy", then "Name copy 2", "Name copy 3"... the first one no case has. The result is
 *  cut so it stays within the device's 255-character limit. */
export function copyName(name: string, taken: string[], max = 255): string {
  const used = new Set(taken.map((t) => t.trim().toLowerCase()));
  for (let i = 1; ; i++) {
    const suffix = i === 1 ? " copy" : ` copy ${i}`;
    const candidate = [...name.trim()].slice(0, Math.max(1, max - suffix.length)).join("") + suffix;
    if (!used.has(candidate.toLowerCase())) return candidate;
  }
}

/** What a merge will do, in words, from the plan the backend computed. */
export function mergeSummary(plan: MergePlan, destName: string): string {
  const parts = [`${plural(plan.added.length, "track")} will be added to “${destName}”`];
  if (plan.already_there > 0) parts.push(`${plural(plan.already_there, "track")} ${plan.already_there === 1 ? "is" : "are"} already in it`);
  if (plan.repeated > 0) parts.push(`${plural(plan.repeated, "track")} ${plan.repeated === 1 ? "appears" : "appear"} in more than one of the cases and is added once`);
  if (plan.stale > 0) parts.push(`${plural(plan.stale, "entry")} in the cases point${plan.stale === 1 ? "s" : ""} at tracks no longer in the library and ${plan.stale === 1 ? "is" : "are"} left out`);
  return parts.join("; ") + ".";
}

/** One sentence per selected track for the copy / move dialog: what will happen to it. */
export function transferLine(id: number, plan: TransferPlan, destName: string, sourceName: string | null, move: boolean): string {
  if (plan.missing.includes(id)) return "is not in the library: left alone";
  if (plan.not_in_source.includes(id)) return `is not in “${sourceName}”: left alone`;
  const there = plan.already_there.includes(id);
  const out = move && plan.to_remove.includes(id);
  if (there) return out ? `is already in “${destName}”: only removed from “${sourceName}”` : `is already in “${destName}”: nothing to do`;
  return out ? `will move from “${sourceName}” to “${destName}”` : `will be added to “${destName}”`;
}
