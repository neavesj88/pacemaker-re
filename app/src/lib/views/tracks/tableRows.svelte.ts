// The rows of the Tracks table, worked out once and shared: the table shows them and the player's
// next / previous walk the same list. Both used to run the whole pipeline (search, filters, a
// multi-key sort of the library) themselves, the player at every track change and on every
// re-render of its buttons. A derived is lazy and cached until something it read changes, so
// nothing is computed while nobody reads it.
//
// An edit swaps a new Track object into the library (a rating, a key from the background
// analysis). When the field it changed is not one the search, filter and sort read, the order
// cannot have changed: the pipeline keeps running on the objects it already had (`input`) and the
// rows are the same places filled with the current objects, which costs a pass over the ids
// instead of a sort (see keepOrder.ts for the rule and its test).
import type { Track } from "$lib/api/types";
import { session } from "$lib/state/session.svelte";
import { orderSurvives } from "./keepOrder";
import { orderFields, tracksViewRows } from "./trackFilter";

class TableRows {
  /** Every track of the open library. */
  all: Track[] = $derived(session.library?.tracks ?? []);
  #fields = $derived(orderFields());
  #kept: Track[] | null = null;
  /** What the pipeline runs on: `all`, or an older copy of it when the edits since did not touch
   *  anything the pipeline reads. */
  #input: Track[] = $derived.by(() => {
    const next = this.all;
    const fields = this.#fields;
    const prev = this.#kept;
    const out = prev !== null && orderSurvives(prev, next, fields) ? prev : next;
    this.#kept = out;
    return out;
  });
  #sorted: Track[] = $derived(tracksViewRows(this.#input));
  /** The table's rows, in order. */
  rows: Track[] = $derived.by(() => {
    const sorted = this.#sorted;
    if (this.#input === this.all) return sorted;
    const now = new Map(this.all.map((t) => [t.track_id, t]));
    return sorted.map((t) => now.get(t.track_id) ?? t);
  });
  /** The same, as ids. */
  ids: number[] = $derived(this.rows.map((t) => t.track_id));
}

export const tableRows = new TableRows();
