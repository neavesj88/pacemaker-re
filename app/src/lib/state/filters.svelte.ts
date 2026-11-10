// The header search, the "needs analysis" filter and the track-table sort. Shared by the
// tracks view and the track list inside a case.
import { session } from "./session.svelte";

export type SortKey = "title" | "artist" | "album" | "genre" | "bpm" | "key" | "play_time_secs";

class Filters {
  query = $state("");
  onlyNeeds = $state(false);
  sortKey = $state<SortKey>("artist");
  sortAsc = $state(true);

  sortBy(key: SortKey) {
    if (this.sortKey === key) this.sortAsc = !this.sortAsc;
    else {
      this.sortKey = key;
      this.sortAsc = true;
    }
  }

  /** A new library was opened: the needs filter is dropped, the search text and sort are kept. */
  reset() {
    this.onlyNeeds = false;
  }
}

export const filters = new Filters();
session.onReset(() => filters.reset());
