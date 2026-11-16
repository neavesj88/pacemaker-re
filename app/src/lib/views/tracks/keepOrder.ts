// When can the Tracks table keep its row order after an edit? An edit (a star rating, a key from
// the background analysis) swaps new Track objects into the library; most of the time the field
// that changed is not one the search, the toolbar filter or the sort reads, so the order cannot
// have changed and the whole search / filter / sort pass over the library (about 94 ms for 20,000
// tracks) would only reproduce it. This file says which fields the pipeline reads and whether a
// change touched one. Pure: no stores, so `npm test` can run it. tableRows.svelte.ts is the user.
import type { Track } from "$lib/api/types";
import type { SortSpec, TrackFilter } from "./trackLogic";

/** Sort columns whose order comes from something other than a field of the track (favourites,
 *  the app-side labels, the chosen case): an edit to the track cannot change them. */
const NOT_A_TRACK_FIELD: ReadonlySet<string> = new Set(["fav", "colour", "energy", "auto_energy", "tags", "cases", "case_order"]);

/** The track fields the toolbar filter and the sort read. `title` is always in: it breaks ties.
 *  The search box is not covered here (its query can read any field): see `orderFields`. */
export function pipelineFields(filter: TrackFilter, sort: readonly SortSpec[]): Set<string> {
  const out = new Set<string>(["title"]);
  if (filter.genre) out.add("genre");
  if (filter.bpmMin !== null || filter.bpmMax !== null) out.add("bpm");
  if (filter.key) out.add("key");
  for (const k of sort) if (!NOT_A_TRACK_FIELD.has(k.field)) out.add(k.field);
  return out;
}

/** True when `next` differs from `prev` only in fields outside `fields`, with the same tracks in
 *  the same places: sorting and filtering `next` would give the order `prev` gave. `fields` null
 *  means "any field may matter" (a search query is active), which never keeps the order. */
export function orderSurvives(prev: readonly Track[], next: readonly Track[], fields: ReadonlySet<string> | null): boolean {
  if (prev === next) return true;
  if (fields === null || prev.length !== next.length) return false;
  for (let i = 0; i < next.length; i++) {
    const a = prev[i];
    const b = next[i];
    if (a === b) continue;
    if (a.track_id !== b.track_id) return false;
    const rec = b as unknown as Record<string, unknown>;
    const old = a as unknown as Record<string, unknown>;
    for (const f of fields) if (old[f] !== rec[f]) return false;
    // a field that appeared or went away is a different shape: do not guess
    if (Object.keys(old).length !== Object.keys(rec).length) return false;
  }
  return true;
}
