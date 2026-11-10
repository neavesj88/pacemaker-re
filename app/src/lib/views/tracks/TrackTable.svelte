<script lang="ts">
  import type { Track } from "$lib/api/types";
  import { mmss } from "$lib/format";
  import { filters, type SortKey } from "$lib/state/filters.svelte";
  import { session } from "$lib/state/session.svelte";
  import Table, { type Column } from "$lib/ui/Table.svelte";

  // The track table. Exported for reuse by other views (the Cases view lists a case with it).
  // `rows` must already be filtered and sorted: see trackFilter.ts.
  let { rows }: { rows: Track[] } = $props();

  const columns: Column[] = [
    { key: "title", label: "Title" },
    { key: "artist", label: "Artist" },
    { key: "album", label: "Album" },
    { key: "genre", label: "Genre" },
    { key: "bpm", label: "BPM", num: true },
    { key: "key", label: "Key" },
    { key: "play_time_secs", label: "Time", num: true },
  ];
</script>

<Table
  {columns}
  {rows}
  rowKey={(t) => t.track_id}
  sortKey={filters.sortKey}
  sortAsc={filters.sortAsc}
  onsort={(k) => filters.sortBy(k as SortKey)}
  rowClass={(t) => (session.needs.has(t.track_id) ? "dim" : "")}
>
  {#snippet cell(t, c)}
    {#if c.key === "title"}
      {t.title}
      {#if session.needs.has(t.track_id)}<span class="badge" title={session.needs.get(t.track_id)}>needs analysis</span>{/if}
    {:else if c.key === "bpm"}
      {t.bpm && !session.needs.has(t.track_id) ? t.bpm.toFixed(2) : "–"}
    {:else if c.key === "play_time_secs"}
      {mmss(t.play_time_secs)}
    {:else}
      {t[c.key as "artist" | "album" | "genre" | "key"]}
    {/if}
  {/snippet}
</Table>

<style>
  .badge {
    margin-left: 6px;
    padding: 0 6px;
    border-radius: 8px;
    font-size: 11px;
    background: var(--accent-soft);
    color: var(--accent);
  }
</style>
