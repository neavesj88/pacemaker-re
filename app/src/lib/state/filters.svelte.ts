// The header search, the "needs analysis" filter and the track-table sort. Shared by the
// tracks view and the track list inside a case.
import { session } from "./session.svelte";

/** How long typing in the search box must pause before the lists are searched again. */
export const SEARCH_PAUSE_MS = 150;

export type SortKey = "title" | "artist" | "album" | "genre" | "bpm" | "key" | "play_time_secs";

export class Filters {
  #text = $state("");
  #applied = $state("");
  #timer: ReturnType<typeof setTimeout> | null = null;

  /** The text in the search box: always what was typed. */
  get query(): string {
    return this.#text;
  }

  set query(v: string) {
    this.#text = v;
    if (this.#timer) clearTimeout(this.#timer);
    // emptied (a reset, or the user cleared it): show everything at once. Typing waits for a
    // pause, because every change runs the search, filters and sort over the whole library.
    if (v === "") {
      this.#timer = null;
      this.#applied = "";
      return;
    }
    this.#timer = setTimeout(() => {
      this.#timer = null;
      this.#applied = this.#text;
    }, SEARCH_PAUSE_MS);
  }

  /** The search the lists are filtered by: `query` once typing has paused for [`SEARCH_PAUSE_MS`]. */
  get applied(): string {
    return this.#applied;
  }

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
