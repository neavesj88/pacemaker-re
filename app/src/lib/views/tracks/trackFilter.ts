// Search + needs-analysis filter + sort for a list of tracks, from the shared filter state.
import type { Track } from "$lib/api/types";
import { filters } from "$lib/state/filters.svelte";
import { session } from "$lib/state/session.svelte";

export function visibleTracks(list: Track[]): Track[] {
  const q = filters.query.trim().toLowerCase();
  const rows = list.filter(
    (t) =>
      (!filters.onlyNeeds || session.needs.has(t.track_id)) &&
      (!q || [t.title, t.artist, t.album, t.genre, t.key].some((f) => f.toLowerCase().includes(q))),
  );
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
