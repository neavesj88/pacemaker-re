// The rows of the Tracks table, worked out once and shared: the table shows them and the player's
// next / previous walk the same list. Both used to run the whole pipeline (search, filters, a
// multi-key sort of the library) themselves, the player at every track change and on every
// re-render of its buttons. A derived is lazy and cached until something it read changes, so
// nothing is computed while nobody reads it.
import type { Track } from "$lib/api/types";
import { session } from "$lib/state/session.svelte";
import { tracksViewRows } from "./trackFilter";

class TableRows {
  /** Every track of the open library. */
  all: Track[] = $derived(session.library?.tracks ?? []);
  /** The table's rows, in order. */
  rows: Track[] = $derived(tracksViewRows(this.all));
  /** The same, as ids. */
  ids: number[] = $derived(this.rows.map((t) => t.track_id));
}

export const tableRows = new TableRows();
