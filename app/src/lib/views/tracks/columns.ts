// The Tracks table's columns, and the (try/catch-guarded) browser storage that remembers which
// are shown, their widths and the sort.
import type { SortField } from "./trackLogic";

export type ColKey = "fav" | "title" | "artist" | "album" | "wave" | "bpm" | "key" | "energy" | "rating" | "plays" | "time" | "genre";

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

export const COLUMNS: ColDef[] = [
  { key: "fav", label: "", width: 34, min: 34, sort: "fav", fixed: true, defaultOn: true },
  { key: "title", label: "Title", width: 190, min: 90, sort: "title", fixed: true, defaultOn: true },
  { key: "artist", label: "Artist", width: 140, min: 70, sort: "artist", defaultOn: true },
  { key: "album", label: "Album", width: 160, min: 70, sort: "album", defaultOn: false },
  { key: "wave", label: "Waveform", width: 100, min: 70, defaultOn: true },
  { key: "bpm", label: "BPM", width: 64, min: 50, num: true, sort: "bpm", defaultOn: true },
  { key: "key", label: "Key", width: 54, min: 46, sort: "key", defaultOn: true },
  // read-only estimate from the audio, kept app-side (views/analysis); blank until analysed
  { key: "energy", label: "Energy (auto)", width: 112, min: 70, sort: "energy", defaultOn: true },
  { key: "rating", label: "Rating", width: 92, min: 90, sort: "rating", defaultOn: true },
  { key: "plays", label: "Plays", width: 48, min: 44, num: true, sort: "times_played", defaultOn: true },
  { key: "time", label: "Time", width: 56, min: 50, num: true, sort: "play_time_secs", defaultOn: true },
  { key: "genre", label: "Genre", width: 100, min: 60, sort: "genre", defaultOn: true },
];

export type ViewPrefs = {
  visible: Record<string, boolean>;
  widths: Record<string, number>;
  sortKey: SortField;
  sortAsc: boolean;
};

const STORAGE_KEY = "pm.tracks.view.v1";

export function defaultPrefs(): ViewPrefs {
  return {
    visible: Object.fromEntries(COLUMNS.map((c) => [c.key, c.defaultOn])),
    widths: Object.fromEntries(COLUMNS.map((c) => [c.key, c.width])),
    sortKey: "artist",
    sortAsc: true,
  };
}

const SORT_FIELDS = new Set(COLUMNS.flatMap((c) => (c.sort ? [c.sort] : [])));

/** Saved prefs merged over the defaults, ignoring anything unknown or malformed. */
export function mergePrefs(saved: unknown): ViewPrefs {
  const prefs = defaultPrefs();
  if (!saved || typeof saved !== "object") return prefs;
  const s = saved as Partial<ViewPrefs>;
  for (const c of COLUMNS) {
    const v = s.visible?.[c.key];
    if (typeof v === "boolean" && !c.fixed) prefs.visible[c.key] = v;
    const w = s.widths?.[c.key];
    if (typeof w === "number" && Number.isFinite(w)) prefs.widths[c.key] = Math.max(c.min, Math.min(800, Math.round(w)));
  }
  if (s.sortKey && SORT_FIELDS.has(s.sortKey)) prefs.sortKey = s.sortKey;
  if (typeof s.sortAsc === "boolean") prefs.sortAsc = s.sortAsc;
  return prefs;
}

export function loadPrefs(): ViewPrefs {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    return mergePrefs(raw ? JSON.parse(raw) : null);
  } catch {
    return defaultPrefs();
  }
}

export function savePrefs(prefs: ViewPrefs) {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(prefs));
  } catch {
    // private window, blocked or full storage: the table still works, it just forgets
  }
}
