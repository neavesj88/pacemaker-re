// State of the Tracks view that outlives the component: the filters of its toolbar, the shown
// columns and sort (remembered in the browser), the favourites, and the track the inspector shows.
// Selection itself is the shared `selection` store.
import { SvelteSet } from "svelte/reactivity";
import { favouriteIds, toggleFavourite } from "$lib/api/cases";
import { jobs } from "$lib/state/jobs.svelte";
import { session } from "$lib/state/session.svelte";
import { toast } from "$lib/state/toast.svelte";
import { COLUMNS, loadPrefs, savePrefs, type ColKey, type ViewPrefs } from "./columns";
import { parseBpm, type SortField, type TrackFilter } from "./trackLogic";

class TracksState {
  prefs = $state<ViewPrefs>(loadPrefs());

  genre = $state("");
  bpmMin = $state("");
  bpmMax = $state("");
  /** "", "compat:8A" or "is:8A" (see trackLogic.KeyFilter). */
  keyFilter = $state("");
  onlyAnalysed = $state(false);

  /** Tracks in the "Favourites" case. */
  favourites = new SvelteSet<number>();
  /** The track the inspector describes (the last one clicked). */
  focusId = $state<number | null>(null);

  filter = $derived<TrackFilter>({
    genre: this.genre,
    bpmMin: parseBpm(this.bpmMin),
    bpmMax: parseBpm(this.bpmMax),
    key: this.keyFilter,
    onlyAnalysed: this.onlyAnalysed,
  });

  /** Columns in table order, with the hidden ones left out. */
  shownColumns = $derived(COLUMNS.filter((c) => c.fixed || this.prefs.visible[c.key]));

  sortBy(field: SortField) {
    if (this.prefs.sortKey === field) this.prefs.sortAsc = !this.prefs.sortAsc;
    else {
      this.prefs.sortKey = field;
      this.prefs.sortAsc = true;
    }
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

  save() {
    savePrefs($state.snapshot(this.prefs));
  }

  clearFilters() {
    this.genre = "";
    this.bpmMin = "";
    this.bpmMax = "";
    this.keyFilter = "";
    this.onlyAnalysed = false;
  }

  /** A different library was opened. */
  reset() {
    this.clearFilters();
    this.favourites.clear();
    this.focusId = null;
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
