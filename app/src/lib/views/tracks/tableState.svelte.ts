// The state one track table needs, as a class so each host has its own: the Tracks view (its
// persisted layout, the shared header search and the shared selection) and the Mix tab (its own
// layout key, its own search box, its own selection). A table's sort, columns, filters, focused
// row and selected rows live here; what is genuinely library-wide (favourites, labels, analysis)
// stays in the shared stores.
import type { SvelteSet } from "svelte/reactivity";
import { Filters } from "$lib/state/filters.svelte";
import { freshLabelFilter, type LabelFilter } from "$lib/state/labels";
import { COLUMNS, FLEX_COLUMNS, loadPrefs, maxWidth, moveColumn, savePrefs, shownColumns as columnsToShow, stepTarget, defaultPrefs, type ColKey, type ViewPrefs } from "./columns";
import { parseBpm, sortClick, type SortField, type SortSpec, type TrackFilter } from "./trackLogic";

export type TableOptions = {
  /** Where the layout (columns, order, widths, sort) is remembered. */
  storageKey: string;
  /** The layout a table starts with before anything is saved. */
  defaults?: () => ViewPrefs;
  /** The search box and the "missing analysis" switch this table filters by. */
  search: Filters;
  /** The selected track ids; the Tracks view passes the app-wide selection. */
  selection: { trackIds: number[] };
  /** The library's favourites, shared by every table. */
  favourites: SvelteSet<number>;
};

export class TableState {
  readonly storageKey: string;
  readonly search: Filters;
  readonly selection: { trackIds: number[] };
  readonly favourites: SvelteSet<number>;
  prefs: ViewPrefs = $state(defaultPrefs());

  genre = $state("");
  bpmMin = $state("");
  bpmMax = $state("");
  /** "", "compat:8A" or "is:8A" (see trackLogic.KeyFilter). */
  keyFilter = $state("");
  onlyAnalysed = $state(false);
  /** Tag, colour and energy conditions (app-side labels, see state/labels.ts). */
  labelFilter = $state<LabelFilter>(freshLabelFilter());
  /** Only tracks flagged by the suspect-grid rules (state/gridSuspects.svelte.ts). */
  onlySuspect = $state(false);
  /** Only the tracks of this case (crate); null = every track. */
  caseId = $state<number | null>(null);
  /** The sort that was in force before a case was chosen, put back when the case is cleared. */
  #sortBeforeCase: SortSpec[] | null = null;
  /** The track the inspector (or the picker's detail) describes: the last one clicked. */
  focusId = $state<number | null>(null);

  filter = $derived<TrackFilter>({
    genre: this.genre,
    bpmMin: parseBpm(this.bpmMin),
    bpmMax: parseBpm(this.bpmMax),
    key: this.keyFilter,
    onlyAnalysed: this.onlyAnalysed,
  });

  /** Columns in table order, with the hidden ones left out ("Case #" only while a case is chosen). */
  shownColumns = $derived(columnsToShow(this.prefs, this.caseId !== null));

  constructor(o: TableOptions) {
    this.storageKey = o.storageKey;
    this.search = o.search;
    this.selection = o.selection;
    this.favourites = o.favourites;
    this.#defaults = o.defaults ?? defaultPrefs;
    this.prefs = loadPrefs(o.storageKey, this.#defaults);
    // the case order only means something while a case is chosen, which a restart is not
    this.prefs.sort = this.persistentSort;
  }

  /** The sort without "case order", which belongs to a chosen case and is not remembered. */
  get persistentSort(): SortSpec[] {
    const keep = this.prefs.sort.filter((k) => k.field !== "case_order");
    return keep.length > 0 ? keep : (this.#sortBeforeCase ?? this.#defaults().sort);
  }
  #defaults: () => ViewPrefs;

  get selectedIds(): number[] {
    return this.selection.trackIds;
  }
  set selectedIds(v: number[]) {
    this.selection.trackIds = v;
  }

  /** A click (or shift-click, `additive`) on a column header. */
  sortBy(field: SortField, additive = false) {
    this.prefs.sort = sortClick(this.prefs.sort, field, additive);
    this.#sortBeforeCase = null; // the user chose a sort: it stays when the case is cleared
    this.save();
  }

  toggleColumn(key: ColKey) {
    this.prefs.visible[key] = !this.prefs.visible[key];
    this.save();
  }

  setWidth(key: ColKey, width: number) {
    const col = COLUMNS.find((c) => c.key === key);
    if (col) this.prefs.widths[key] = Math.max(col.min, Math.min(maxWidth(key), Math.round(width)));
  }

  /** Keeps a Title/Artist column at the width it was dragged to (`fillWidth` then leaves it alone). */
  pinColumn(key: ColKey) {
    if (FLEX_COLUMNS.includes(key) && !this.prefs.pinned.includes(key)) this.prefs.pinned = [...this.prefs.pinned, key];
  }

  /** Moves a column to just before `before` (the end when null). */
  moveColumn(key: ColKey, before: ColKey | null) {
    this.prefs.order = moveColumn(this.prefs.order, key, before);
    this.save();
  }

  /** Moves a column one step along among the shown columns (the keyboard way); false at the edge. */
  stepColumn(key: ColKey, step: -1 | 1): boolean {
    const t = stepTarget(
      this.shownColumns.map((c) => c.key),
      key,
      step,
    );
    if (!t.ok) return false;
    this.moveColumn(key, t.before);
    return true;
  }

  /** Back to the default columns, order and widths (the sort and the settings stay). */
  resetColumns() {
    const d = this.#defaults();
    this.prefs.visible = d.visible;
    this.prefs.widths = d.widths;
    this.prefs.order = d.order;
    this.prefs.pinned = [];
    this.save();
  }

  setFillWidth(on: boolean) {
    this.prefs.fillWidth = on;
    if (on) this.prefs.pinned = [];
    this.save();
  }

  setCaseOrderOnPick(on: boolean) {
    this.prefs.caseOrderOnPick = on;
    this.save();
  }

  /** Chooses a case (null = every track). With the setting on, a chosen case is shown in its own
   *  order and the previous sort comes back when it is cleared. */
  setCase(id: number | null) {
    if (id === this.caseId) return;
    const was = this.caseId;
    this.caseId = id;
    if (!this.prefs.caseOrderOnPick) return;
    if (id !== null && was === null) {
      this.#sortBeforeCase = $state.snapshot(this.prefs.sort);
      this.prefs.sort = [{ field: "case_order", asc: true }];
    } else if (id === null && this.#sortBeforeCase) {
      this.prefs.sort = this.#sortBeforeCase;
      this.#sortBeforeCase = null;
    }
  }

  save() {
    savePrefs($state.snapshot(this.prefs), this.storageKey);
  }

  /** The toolbar's own filters; the search box and "Missing" are the table's `search`. */
  clearFilters() {
    this.genre = "";
    this.bpmMin = "";
    this.bpmMax = "";
    this.keyFilter = "";
    this.onlyAnalysed = false;
    this.labelFilter = freshLabelFilter();
    this.onlySuspect = false;
    this.setCase(null);
  }

  /** Every condition that hides tracks: the toolbar, "Missing" and the search box. */
  clearAllFilters() {
    this.clearFilters();
    this.search.onlyNeeds = false;
    this.search.query = "";
  }
}
