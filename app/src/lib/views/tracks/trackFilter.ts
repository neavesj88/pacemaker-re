// Search + needs-analysis filter + sort for a list of tracks, from the shared filter state.
//
// The search box understands the query language of query.ts (words, `field:value`, ranges,
// comparisons, `key:~8A`, tags, `-not`, `OR`). `searchTracks` is the search every list uses;
// `tracksViewRows` is the whole pipeline of the Tracks table (search, toolbar filters, label
// filter, sort), kept in one place so that what the table shows and what the player's
// next / previous walk through can never differ.
import type { Track } from "$lib/api/types";
import { filters } from "$lib/state/filters.svelte";
import { suspects } from "$lib/state/gridSuspects.svelte";
import { energy } from "$lib/state/energy.svelte";
import { filterByLabels } from "$lib/state/labels";
import { labels } from "$lib/state/labels.svelte";
import { session } from "$lib/state/session.svelte";
import { compileQuery, type QueryContext } from "./query";
import { filterTracks, sortTracksBy } from "./trackLogic";
import { tracksState } from "./tracksState.svelte";

/** What the query language needs besides the track: labels, favourites, tracks needing analysis. */
export function queryContext(): QueryContext {
  return {
    labelOf: (id) => labels.get(id),
    needs: session.needs,
    isFavourite: (id) => tracksState.favourites.has(id),
    now: Date.now(),
  };
}

/** The header search and the "needs analysis" filter, in the list's own order. */
export function searchTracks(list: Track[]): Track[] {
  const q = compileQuery(filters.query);
  const cx = queryContext();
  return list.filter((t) => (!filters.onlyNeeds || session.needs.has(t.track_id)) && (q.empty || q.test(t, cx)));
}

export function visibleTracks(list: Track[]): Track[] {
  const rows = searchTracks(list);
  const dir = filters.sortAsc ? 1 : -1;
  const key = filters.sortKey;
  rows.sort((a, b) => {
    const x = a[key];
    const y = b[key];
    if (typeof x === "number" && typeof y === "number") return (x - y) * dir;
    return String(x).localeCompare(String(y), undefined, { sensitivity: "base" }) * dir;
  });
  return rows;
}

/** The rows of the Tracks table, in order: the header search, then the toolbar's filters, then
 *  the label filter, then the "Suspect grid" filter (flags come from the grid area's rules), then
 *  the sort (every key, in order). */
export function tracksViewRows(all: Track[]): Track[] {
  const found = filterTracks(searchTracks(all), tracksState.filter, session.needs);
  const labelled = filterByLabels(found, tracksState.labelFilter, (id) => labels.get(id));
  return sortTracksBy(
    tracksState.onlySuspect ? labelled.filter((t) => suspects.byId.has(t.track_id)) : labelled,
    tracksState.prefs.sort,
    (id) => tracksState.favourites.has(id),
    (id) => labels.get(id),
    (id) => energy.of(id),
  );
}
