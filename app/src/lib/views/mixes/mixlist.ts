// Pure logic of the mix list: search, sort, selection, copy names and the words for bulk
// confirmations and reports. No Svelte and no Tauri here, so scripts/test-mixedit.mjs runs it
// under plain node.
import type { ExportPlanItem, MixMeta, MixSummary, OnExisting } from "$lib/api/mixes";

/** Longest mix name the library accepts (`MAX_MIX_NAME_CHARS`). */
export const MAX_NAME = 255;

export type SortKey = "name" | "play_time_secs" | "track_count" | "date" | "rating" | "genre";

export const SORT_LABELS: Record<SortKey, string> = {
  name: "Name",
  play_time_secs: "Length",
  track_count: "Tracks",
  date: "Modified",
  rating: "Rating",
  genre: "Genre",
};

/** The order a key starts in when chosen: text ascending, numbers and dates biggest first. */
export const defaultAscending = (key: SortKey) => key === "name" || key === "genre";

// ---------------------------------------------------------------------------------------------
// Settings of the list (remembered by listPrefs.svelte.ts)
// ---------------------------------------------------------------------------------------------

export type ListPrefs = {
  sortKey: SortKey;
  asc: boolean;
  /** Search also looks in genre, producer, remixer, comments and year, not only the name. */
  deepSearch: boolean;
  /** What a bulk export does when the file it would write already exists. Default: never overwrite. */
  onExisting: OnExisting;
};

export const DEFAULT_LIST_PREFS: ListPrefs = { sortKey: "name", asc: true, deepSearch: true, onExisting: "number" };

/** Settings read back from storage: anything missing or not one of the known values falls back
 * to the default, so a corrupt or old entry never breaks the list. */
export function mergeListPrefs(raw: unknown): ListPrefs {
  const o = (raw && typeof raw === "object" ? raw : {}) as Record<string, unknown>;
  const d = DEFAULT_LIST_PREFS;
  return {
    sortKey: typeof o.sortKey === "string" && o.sortKey in SORT_LABELS ? (o.sortKey as SortKey) : d.sortKey,
    asc: typeof o.asc === "boolean" ? o.asc : d.asc,
    deepSearch: typeof o.deepSearch === "boolean" ? o.deepSearch : d.deepSearch,
    onExisting: o.onExisting === "number" || o.onExisting === "replace" || o.onExisting === "skip" ? o.onExisting : d.onExisting,
  };
}

// ---------------------------------------------------------------------------------------------
// Search
// ---------------------------------------------------------------------------------------------

/** Lower case with accents removed, so "cafe" finds "Café". */
export const fold = (s: string) => s.normalize("NFD").replace(/[̀-ͯ]/g, "").toLowerCase();

/** The words of a query: split on white space, folded, empty ones dropped. */
export const terms = (query: string) => fold(query).split(/\s+/).filter(Boolean);

/** True when every word of the query is in the mix's name, or, with `deep`, also in its genre,
 * producer, remixer, comments or year. An empty query matches everything. */
export function matches(m: MixSummary, meta: MixMeta | undefined, query: string, deep: boolean): boolean {
  const words = terms(query);
  if (words.length === 0) return true;
  const hay = [m.name];
  if (deep && meta) hay.push(meta.genre, meta.producer, meta.remixer, meta.comments, meta.year > 0 ? String(meta.year) : "");
  const text = fold(hay.join("\n"));
  return words.every((w) => text.includes(w));
}

export function filterMixes(list: MixSummary[], metaOf: (id: number) => MixMeta | undefined, query: string, deep: boolean): MixSummary[] {
  return terms(query).length === 0 ? list : list.filter((m) => matches(m, metaOf(m.id), query, deep));
}

// ---------------------------------------------------------------------------------------------
// Sort
// ---------------------------------------------------------------------------------------------

const byName = (a: string, b: string) => a.localeCompare(b, undefined, { numeric: true, sensitivity: "base" });

/** A sorted copy. Equal keys fall back to the name (always ascending) so the order is stable and
 * predictable; a mix without library facts yet sorts as 0 / empty. */
export function sortMixes(list: MixSummary[], key: SortKey, asc: boolean, metaOf: (id: number) => MixMeta | undefined): MixSummary[] {
  const dir = asc ? 1 : -1;
  const num = (m: MixSummary): number => {
    switch (key) {
      case "play_time_secs":
        return m.play_time_secs;
      case "track_count":
        return m.track_count;
      case "date":
        return metaOf(m.id)?.dateModified ?? 0;
      case "rating":
        return metaOf(m.id)?.rating ?? 0;
      default:
        return 0;
    }
  };
  return [...list].sort((a, b) => {
    let c = 0;
    if (key === "name") c = byName(a.name, b.name) * dir;
    else if (key === "genre") c = byName(metaOf(a.id)?.genre ?? "", metaOf(b.id)?.genre ?? "") * dir;
    else c = (num(a) - num(b)) * dir;
    return c || byName(a.name, b.name) || a.id - b.id;
  });
}

// ---------------------------------------------------------------------------------------------
// Selection
// ---------------------------------------------------------------------------------------------

export type Selection = { selected: ReadonlySet<number>; anchor: number | null };

/** What a click on a row does. Plain click: that row alone. Ctrl / Cmd: toggles it. Shift: every
 * row from the anchor (the last row clicked without Shift) to this one, shown order, replacing
 * the selection unless Ctrl is also held. `rows` are the ids on screen, in order. */
export function clickRow(state: Selection, rows: number[], id: number, mods: { shift: boolean; ctrl: boolean }): Selection {
  if (mods.shift && state.anchor !== null && rows.includes(state.anchor)) {
    const a = rows.indexOf(state.anchor);
    const b = rows.indexOf(id);
    const [lo, hi] = a <= b ? [a, b] : [b, a];
    const range = rows.slice(lo, hi + 1);
    return { selected: new Set(mods.ctrl ? [...state.selected, ...range] : range), anchor: state.anchor };
  }
  if (mods.ctrl) {
    const next = new Set(state.selected);
    if (next.has(id)) next.delete(id);
    else next.add(id);
    return { selected: next, anchor: id };
  }
  return { selected: new Set([id]), anchor: id };
}

/** Ticking a row's box adds or removes just that row. */
export function toggleRow(state: Selection, id: number): Selection {
  const next = new Set(state.selected);
  if (next.has(id)) next.delete(id);
  else next.add(id);
  return { selected: next, anchor: id };
}

/** The selection limited to rows that are still on screen: a search that hides a ticked row
 * un-ticks it, so a bulk action never touches a mix the user cannot see. */
export function prune(selected: ReadonlySet<number>, shown: number[]): Set<number> {
  const on = new Set(shown);
  return new Set([...selected].filter((id) => on.has(id)));
}

// ---------------------------------------------------------------------------------------------
// Names of copies (the Rust `unique_name` / `copy_name`, for the default shown in the dialog)
// ---------------------------------------------------------------------------------------------

export function uniqueName(taken: Iterable<string>, base: string): string {
  const used = new Set(taken);
  const cut = (s: string, room: number) => [...s].slice(0, room).join("").trimEnd();
  const first = cut(base.trim(), MAX_NAME);
  if (!used.has(first)) return first;
  for (let n = 2; ; n++) {
    const suffix = ` (${n})`;
    const name = cut(base.trim(), MAX_NAME - suffix.length) + suffix;
    if (!used.has(name)) return name;
  }
}

export const copyName = (taken: Iterable<string>, original: string) => uniqueName(taken, `${original.trim()} copy`);

// ---------------------------------------------------------------------------------------------
// Words for confirmations and reports
// ---------------------------------------------------------------------------------------------

const plural = (n: number, one: string, many = `${one}s`) => `${n} ${n === 1 ? one : many}`;

export const clockOf = (secs: number) => {
  const s = Math.max(0, Math.round(secs));
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
};

/** One line of the delete confirmation: the mix and what it holds. */
export const deleteLine = (m: MixSummary) => `“${m.name}”: ${plural(m.track_count, "track")}, ${clockOf(m.play_time_secs)}`;

/** One line of the export confirmation, saying in words what will happen to which file. */
export function planLine(p: ExportPlanItem): string {
  switch (p.action) {
    case "replace":
      return `“${p.name}” → ${p.fileName}  (REPLACES the file of that name that is there now)`;
    case "skip":
      return `“${p.name}” → not exported (${p.fileName} already exists and is kept)`;
    default:
      return `“${p.name}” → ${p.fileName}  (new file)`;
  }
}

export type DeleteResult = { deleted: { id: number; name: string }[]; failed: { id: number; name: string; error: string }[] };

/** The toast after a bulk delete: what was deleted, and each mix that was kept with the reason. */
export function deleteSummary(r: DeleteResult): { text: string; ok: boolean } {
  const head = r.deleted.length ? `Deleted ${plural(r.deleted.length, "mix", "mixes")}.` : "Nothing was deleted.";
  if (r.failed.length === 0) return { text: head, ok: true };
  const shown = r.failed.slice(0, 4).map((f) => `“${f.name}”: ${f.error}`);
  const more = r.failed.length > shown.length ? ` and ${r.failed.length - shown.length} more` : "";
  return { text: `${head} Kept ${r.failed.length}: ${shown.join("; ")}${more}.`, ok: false };
}

export type ExportResult = {
  written: { id: number; name: string; path: string }[];
  skipped: { id: number; name: string }[];
  failed: { id: number; name: string; error: string }[];
};

export function exportSummary(r: ExportResult, dir: string): { text: string; ok: boolean } {
  const bits: string[] = [];
  bits.push(r.written.length ? `Exported ${plural(r.written.length, "mix", "mixes")} to ${dir}.` : "Nothing was exported.");
  if (r.skipped.length) bits.push(`Skipped ${r.skipped.length} (file already there).`);
  if (r.failed.length) {
    const shown = r.failed.slice(0, 4).map((f) => `“${f.name}”: ${f.error}`);
    const more = r.failed.length > shown.length ? ` and ${r.failed.length - shown.length} more` : "";
    bits.push(`Failed ${r.failed.length}: ${shown.join("; ")}${more}.`);
  }
  return { text: bits.join(" "), ok: r.failed.length === 0 };
}

/** The deep-link keys that carry track ids into the Mixes view: with or without the area prefix. */
export const ADD_KEYS = ["addTracks", "mixes.addTracks"] as const;

/** The tracks a deep link asks to add. A sender may pass the same list under both keys (the Tracks view
 * does), so the first list found is the request: reading both added every track twice. */
export function addIdsFrom(params: Record<string, unknown>): number[] {
  for (const key of ADD_KEYS) {
    const ids = params[key];
    if (Array.isArray(ids)) return ids.filter((x): x is number => typeof x === "number");
  }
  return [];
}

/** What a reload of the open library means for the mix open in the editor: `open` is the `dateModified` the
 * document was read with, `inLibrary` the one the library row has now (null = the mix is gone). A restore or a
 * Sync that replaces a mix changes it behind the editor, and a later Save would write the old text over it. */
export type OpenMixVerdict = "same" | "reload" | "changed" | "removed" | "gone";

export function openMixVerdict(open: number | undefined, inLibrary: number | null, dirty: boolean): OpenMixVerdict {
  if (inLibrary === null) return dirty ? "gone" : "removed";
  if (open === undefined || open === inLibrary) return "same";
  return dirty ? "changed" : "reload";
}
