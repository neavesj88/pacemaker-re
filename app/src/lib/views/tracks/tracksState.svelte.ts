// State of the Tracks view that outlives the component: the filters of its toolbar, the shown
// columns, their order and the sort (remembered in the browser), the favourites, the saved
// filter presets, and the track the inspector shows. Selection itself is the shared `selection`
// store; the search box is the shared `filters.query`.
//
// Remembered view. When the setting is on, the search text, filters and sort of each library are
// kept in the app-side views file (commands/tracks/views.rs) and put back the next time that
// library is opened. Saving is debounced and only starts once the library's own state has been
// read, so a library never starts by overwriting itself with defaults.
import { SvelteSet } from "svelte/reactivity";
import { favouriteIds, toggleFavourite } from "$lib/api/cases";
import { tracksViewsDeletePreset, tracksViewsLoad, tracksViewsSavePreset, tracksViewsSetState, type ViewPreset } from "$lib/api/tracks";
import { filters } from "$lib/state/filters.svelte";
import { freshLabelFilter, type LabelFilter } from "$lib/state/labels";
import { jobs } from "$lib/state/jobs.svelte";
import { session } from "$lib/state/session.svelte";
import { toast } from "$lib/state/toast.svelte";
import { COLUMNS, loadPrefs, moveColumn, savePrefs, shownColumns as columnsToShow, stepTarget, defaultPrefs, type ColKey, type ViewPrefs } from "./columns";
import { parseBpm, sortClick, type SortField, type TrackFilter } from "./trackLogic";
import { emptyViewState, sanitiseViewState, type ViewState } from "./viewState";

/** How long a change waits before it is written, so typing a query writes once. */
const SAVE_DELAY_MS = 600;

class TracksState {
  prefs = $state<ViewPrefs>(loadPrefs());

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

  /** Tracks in the "Favourites" case. */
  favourites = new SvelteSet<number>();
  /** The track the inspector describes (the last one clicked). */
  focusId = $state<number | null>(null);

  /** Saved filter presets of the open library (app-side, not on the device). */
  presets = $state.raw<ViewPreset[]>([]);
  /** Set when the saved-filters file could not be read. */
  viewsNotice = $state("");
  /** The library whose saved views have been read; nothing is written until it is the open one. */
  viewsRoot = $state<string | null>(null);

  filter = $derived<TrackFilter>({
    genre: this.genre,
    bpmMin: parseBpm(this.bpmMin),
    bpmMax: parseBpm(this.bpmMax),
    key: this.keyFilter,
    onlyAnalysed: this.onlyAnalysed,
  });

  /** Columns in table order, with the hidden ones left out. */
  shownColumns = $derived(columnsToShow(this.prefs));

  #lastJson = "";
  #timer: ReturnType<typeof setTimeout> | null = null;
  #pending: { root: string; state: ViewState } | null = null;
  #saveFailed = false;

  /** A click (or shift-click, `additive`) on a column header. */
  sortBy(field: SortField, additive = false) {
    this.prefs.sort = sortClick(this.prefs.sort, field, additive);
    this.save();
  }

  toggleColumn(key: ColKey) {
    this.prefs.visible[key] = !this.prefs.visible[key];
    this.save();
  }

  setWidth(key: ColKey, width: number) {
    const col = COLUMNS.find((c) => c.key === key);
    if (col) this.prefs.widths[key] = Math.max(col.min, Math.min(800, Math.round(width)));
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

  /** Back to the default columns, order and widths (the sort and the two settings stay). */
  resetColumns() {
    const d = defaultPrefs();
    this.prefs.visible = d.visible;
    this.prefs.widths = d.widths;
    this.prefs.order = d.order;
    this.save();
  }

  setRememberView(on: boolean) {
    this.prefs.rememberView = on;
    this.save();
    if (on) this.noteChange();
  }

  setFindClearsFilters(on: boolean) {
    this.prefs.findClearsFilters = on;
    this.save();
  }

  save() {
    savePrefs($state.snapshot(this.prefs));
  }

  /** The toolbar's own filters; the search box and "Missing" are the shared `filters`. */
  clearFilters() {
    this.genre = "";
    this.bpmMin = "";
    this.bpmMax = "";
    this.keyFilter = "";
    this.onlyAnalysed = false;
    this.labelFilter = freshLabelFilter();
    this.onlySuspect = false;
  }

  /** Every condition that hides tracks: the toolbar, "Missing" and the search box. */
  clearAllFilters() {
    this.clearFilters();
    filters.onlyNeeds = false;
    filters.query = "";
  }

  // ---- saved views ----------------------------------------------------------------------

  /** The search, filters and (with `withSort`) sort as one object, for a preset or the
   *  remembered view. */
  captureState(withSort = false): ViewState {
    return {
      query: filters.query,
      onlyNeeds: filters.onlyNeeds,
      onlyAnalysed: this.onlyAnalysed,
      onlySuspect: this.onlySuspect,
      genre: this.genre,
      bpmMin: this.bpmMin,
      bpmMax: this.bpmMax,
      keyFilter: this.keyFilter,
      labelFilter: $state.snapshot(this.labelFilter),
      ...(withSort ? { sort: $state.snapshot(this.prefs.sort) } : {}),
    };
  }

  /** Puts a saved state on screen. Everything it names is replaced; the sort changes only when
   *  the state carries one. */
  applyState(s: ViewState) {
    filters.query = s.query;
    filters.onlyNeeds = s.onlyNeeds;
    this.onlyAnalysed = s.onlyAnalysed;
    this.onlySuspect = s.onlySuspect;
    this.genre = s.genre;
    this.bpmMin = s.bpmMin;
    this.bpmMax = s.bpmMax;
    this.keyFilter = s.keyFilter;
    this.labelFilter = { ...s.labelFilter, tags: [...s.labelFilter.tags], colours: [...s.labelFilter.colours] };
    if (s.sort) {
      this.prefs.sort = s.sort.map((k) => ({ ...k }));
      this.save();
    }
  }

  /** Reads the saved views of the library that was just opened and, when the setting is on, puts
   *  its remembered view back. */
  async loadViews(root: string) {
    this.viewsRoot = null;
    this.presets = [];
    this.viewsNotice = "";
    this.#saveFailed = false;
    try {
      const v = await tracksViewsLoad(root);
      if (session.library?.root !== root) return; // another library was opened meanwhile
      this.presets = v.presets;
      this.viewsNotice = v.notice ?? "";
      if (this.prefs.rememberView) {
        const s = sanitiseViewState(v.state);
        if (s) this.applyState(s);
      }
      this.#lastJson = JSON.stringify(this.captureState(true));
      this.viewsRoot = root;
    } catch (e) {
      toast.show(`Could not read the saved filters: ${e}`, "bad");
    }
  }

  /** Called whenever any part of the view may have changed (from an effect, so the reads here are
   *  what it tracks). Schedules a write when the remembered view differs from what was last saved. */
  noteChange() {
    const root = this.viewsRoot;
    if (!root || !this.prefs.rememberView || session.library?.root !== root) return;
    const state = this.captureState(true);
    const json = JSON.stringify(state);
    if (json === this.#lastJson) return;
    this.#lastJson = json;
    this.#pending = { root, state };
    if (this.#timer) clearTimeout(this.#timer);
    this.#timer = setTimeout(() => void this.flushViews(), SAVE_DELAY_MS);
  }

  /** Writes a waiting change now. */
  async flushViews() {
    if (this.#timer) clearTimeout(this.#timer);
    this.#timer = null;
    const p = this.#pending;
    this.#pending = null;
    if (!p) return;
    try {
      await tracksViewsSetState(p.root, p.state);
    } catch (e) {
      // once per library: a read-only file would otherwise complain at every keystroke
      if (!this.#saveFailed) toast.show(`Could not remember the filters and sort: ${e}`, "bad");
      this.#saveFailed = true;
    }
  }

  /** Saves the current search and filters (and the sort when `withSort`) as a preset. Returns
   *  true when saved. A name that exists is replaced only with `replace`. */
  async savePreset(name: string, withSort: boolean, replace: boolean): Promise<boolean> {
    const root = session.library?.root;
    if (!root) return false;
    try {
      const v = await tracksViewsSavePreset(root, name, this.captureState(withSort), replace);
      if (session.library?.root !== root) return false;
      this.presets = v.presets;
      this.viewsNotice = v.notice ?? "";
      return true;
    } catch (e) {
      toast.show(`Could not save the preset: ${e}`, "bad");
      return false;
    }
  }

  async deletePreset(id: string): Promise<boolean> {
    const root = session.library?.root;
    if (!root) return false;
    try {
      const v = await tracksViewsDeletePreset(root, id);
      if (session.library?.root !== root) return false;
      this.presets = v.presets;
      return true;
    } catch (e) {
      toast.show(`Could not delete the preset: ${e}`, "bad");
      return false;
    }
  }

  /** What a preset holds, read safely (an unreadable one holds nothing). */
  presetState(p: ViewPreset): ViewState {
    return sanitiseViewState(p.state) ?? emptyViewState();
  }

  /** Puts a preset's search and filters on screen. */
  applyPreset(p: ViewPreset): boolean {
    const s = sanitiseViewState(p.state);
    if (!s) {
      toast.show(`The preset “${p.name}” could not be read.`, "bad");
      return false;
    }
    this.applyState(s);
    return true;
  }

  /** A different library was opened (or the same one again). */
  reset() {
    void this.flushViews(); // a change waiting for the library just left is still written to it
    this.clearFilters();
    this.favourites.clear();
    this.focusId = null;
    const root = session.library?.root;
    if (root) void this.loadViews(root);
    else {
      this.viewsRoot = null;
      this.presets = [];
    }
  }

  async loadFavourites(root: string) {
    try {
      const ids = await favouriteIds(root);
      if (session.library?.root !== root) return; // another library was opened meanwhile
      this.favourites.clear();
      for (const id of ids) this.favourites.add(id);
    } catch (e) {
      toast.show(`Could not read the favourites: ${e}`, "bad");
    }
  }

  /** Makes every track in `ids` a favourite (`on`) or not, showing the change straight away and
   *  undoing it for a track the device refuses. */
  async setFavourite(ids: number[], on: boolean) {
    const root = session.library?.root;
    if (!root) return;
    const todo = ids.filter((id) => this.favourites.has(id) !== on);
    if (todo.length === 0) return;
    if (!jobs.canWrite) {
      toast.show("Favourites can't be changed while a job runs or the library is read-only.", "bad");
      return;
    }
    for (const id of todo) {
      if (on) this.favourites.add(id);
      else this.favourites.delete(id);
    }
    for (const [i, id] of todo.entries()) {
      try {
        const now = await toggleFavourite(root, id);
        if (now) this.favourites.add(id);
        else this.favourites.delete(id);
      } catch (e) {
        for (const undo of todo.slice(i)) {
          if (on) this.favourites.delete(undo);
          else this.favourites.add(undo);
        }
        toast.show(`Could not change the favourites: ${e}`, "bad");
        return;
      }
    }
  }
}

export const tracksState = new TracksState();
session.onReset(() => tracksState.reset());
