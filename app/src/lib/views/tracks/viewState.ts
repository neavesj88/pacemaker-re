// What a saved filter preset and the remembered view of a library hold: the search text, the
// toolbar filters, the label filter and (optionally) the sort. Pure, so the rules that read it
// back from disk run under plain node (viewState.test.mjs). The backend stores the object
// as it is (commands/tracks/views.rs); everything read from it passes through
// `sanitiseViewState`, so a hand-edited or newer file cannot put junk into the filters.
import { describeLabelFilter, freshLabelFilter, isColour, MAX_ENERGY, MAX_TAG_CHARS, MAX_TAGS, MIN_ENERGY, type LabelFilter } from "../../state/labels.ts";
import { cleanSort } from "./columns.ts";
import type { SortSpec } from "./trackLogic.ts";

export type ViewState = {
  /** The header search box, including any typed query (`bpm:120-128 key:~8A`). */
  query: string;
  /** "Missing": only tracks whose beat grid is missing or unreadable. */
  onlyNeeds: boolean;
  /** "Analysed": only tracks with a beat grid. */
  onlyAnalysed: boolean;
  /** "Suspect grid": only tracks whose beat grid failed the grid check. Absent in files saved before it existed. */
  onlySuspect: boolean;
  genre: string;
  bpmMin: string;
  bpmMax: string;
  /** "", "compat:8A" or "is:8A". */
  keyFilter: string;
  labelFilter: LabelFilter;
  /** Present when the state also fixes the sort. */
  sort?: SortSpec[];
};

export const MAX_QUERY_CHARS = 500;

export function emptyViewState(): ViewState {
  return { query: "", onlyNeeds: false, onlyAnalysed: false, onlySuspect: false, genre: "", bpmMin: "", bpmMax: "", keyFilter: "", labelFilter: freshLabelFilter() };
}

const str = (v: unknown, max: number): string => (typeof v === "string" ? v.slice(0, max) : "");
const bool = (v: unknown): boolean => v === true;

function energy(v: unknown): number | null {
  return typeof v === "number" && Number.isInteger(v) && v >= MIN_ENERGY && v <= MAX_ENERGY ? v : null;
}

/** A state read from storage made safe: wrong types become the empty value, unknown colours and
 *  over-long text are dropped, the key filter must be a real one. Null when `raw` is not an object. */
export function sanitiseViewState(raw: unknown): ViewState | null {
  if (!raw || typeof raw !== "object" || Array.isArray(raw)) return null;
  const r = raw as Record<string, unknown>;
  const lf = r.labelFilter && typeof r.labelFilter === "object" ? (r.labelFilter as Record<string, unknown>) : {};
  const tags = Array.isArray(lf.tags) ? lf.tags.filter((t): t is string => typeof t === "string" && t.trim() !== "" && t.length <= MAX_TAG_CHARS).slice(0, MAX_TAGS) : [];
  const colours = Array.isArray(lf.colours) ? [...new Set(lf.colours.filter(isColour))] : [];
  const out: ViewState = {
    query: str(r.query, MAX_QUERY_CHARS),
    onlyNeeds: bool(r.onlyNeeds),
    onlyAnalysed: bool(r.onlyAnalysed),
    onlySuspect: bool(r.onlySuspect),
    genre: str(r.genre, 200),
    bpmMin: str(r.bpmMin, 12),
    bpmMax: str(r.bpmMax, 12),
    keyFilter: typeof r.keyFilter === "string" && /^(?:(?:compat|is):(?:1[0-2]|[1-9])[AB])?$/.test(r.keyFilter) ? r.keyFilter : "",
    labelFilter: { tags, tagMode: lf.tagMode === "all" ? "all" : "any", colours, energyMin: energy(lf.energyMin), energyMax: energy(lf.energyMax) },
  };
  // "Analysed" and "Missing" are opposites; a file that has both on keeps neither
  if (out.onlyNeeds && out.onlyAnalysed) out.onlyNeeds = out.onlyAnalysed = false;
  const sort = cleanSort(r.sort);
  if (sort) out.sort = sort;
  return out;
}

/** Same filters and sort? (Key order in the JSON does not matter.) */
export function sameViewState(a: ViewState, b: ViewState): boolean {
  return JSON.stringify(canonical(a)) === JSON.stringify(canonical(b));
}

function canonical(s: ViewState) {
  return [s.query, s.onlyNeeds, s.onlyAnalysed, s.onlySuspect, s.genre, s.bpmMin, s.bpmMax, s.keyFilter, s.labelFilter.tags, s.labelFilter.tagMode, s.labelFilter.colours, s.labelFilter.energyMin, s.labelFilter.energyMax, s.sort ?? null];
}

/** True when the state would show every track (no search, no filter). The sort does not count. */
export function viewStateIsClear(s: ViewState): boolean {
  return sameViewState({ ...s, sort: undefined }, emptyViewState());
}

/** One line saying what the state filters by, for the preset list and for confirmations. */
export function describeViewState(s: ViewState, sortLabel?: (spec: SortSpec) => string): string {
  const parts: string[] = [];
  if (s.query.trim()) parts.push(`search “${s.query.trim()}”`);
  if (s.genre) parts.push(`genre ${s.genre}`);
  if (s.bpmMin || s.bpmMax) parts.push(`BPM ${s.bpmMin || "any"} to ${s.bpmMax || "any"}`);
  if (s.keyFilter) {
    const [mode, code] = s.keyFilter.split(":");
    parts.push(mode === "is" ? `key ${code}` : `keys compatible with ${code}`);
  }
  const labels = describeLabelFilter(s.labelFilter);
  if (labels) parts.push(labels);
  if (s.onlyAnalysed) parts.push("only analysed");
  if (s.onlyNeeds) parts.push("only missing a beat grid");
  if (s.onlySuspect) parts.push("only suspect beat grids");
  if (s.sort && s.sort.length > 0) parts.push(`sorted by ${s.sort.map((k) => (sortLabel ? sortLabel(k) : k.field) + (k.asc ? " ↑" : " ↓")).join(", ")}`);
  return parts.length > 0 ? parts.join("; ") : "no filters";
}
