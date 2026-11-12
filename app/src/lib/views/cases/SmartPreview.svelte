<script lang="ts">
  import { mmss } from "$lib/format";
  import { session } from "$lib/state/session.svelte";
  import Table, { type Column } from "$lib/ui/Table.svelte";
  import { searchTracks } from "../tracks/trackFilter";
  import type { Track } from "$lib/api/types";

  // The tracks a smart case matches, in the order the smart case gives them (rank column), not in
  // the track table's own sort: that is what a device copy would hold. The header search still
  // narrows the list. `values` is what the order is based on when the table has no column for it.
  let { ids, values = [], valueLabel = "" }: { ids: number[]; values?: string[]; valueLabel?: string } = $props();

  type Row = { rank: number; t: Track; v: string };

  let rows = $derived.by(() => {
    const byId = new Map((session.library?.tracks ?? []).map((t) => [t.track_id, t]));
    const ranked: Row[] = [];
    ids.forEach((id, i) => {
      const t = byId.get(id);
      if (t) ranked.push({ rank: i + 1, t, v: values[i] ?? "" });
    });
    const keep = new Set(searchTracks(ranked.map((r) => r.t)).map((t) => t.track_id));
    return ranked.filter((r) => keep.has(r.t.track_id));
  });

  let columns = $derived<Column[]>([
    { key: "rank", label: "#", num: true },
    ...(values.length > 0 ? [{ key: "v", label: valueLabel }] : []),
    { key: "title", label: "Title" },
    { key: "artist", label: "Artist" },
    { key: "album", label: "Album" },
    { key: "genre", label: "Genre" },
    { key: "bpm", label: "BPM", num: true },
    { key: "key", label: "Key" },
    { key: "play_time_secs", label: "Time", num: true },
  ]);
</script>

<Table {columns} {rows} rowKey={(r) => r.t.track_id} rowClass={(r) => (session.needs.has(r.t.track_id) ? "dim" : "")}>
  {#snippet cell(r, c)}
    {#if c.key === "rank"}
      <span class="muted">{r.rank}</span>
    {:else if c.key === "v"}
      {r.v}
    {:else if c.key === "bpm"}
      {r.t.bpm && !session.needs.has(r.t.track_id) ? r.t.bpm.toFixed(2) : "–"}
    {:else if c.key === "play_time_secs"}
      {mmss(r.t.play_time_secs)}
    {:else}
      {r.t[c.key as "title" | "artist" | "album" | "genre" | "key"]}
    {/if}
  {/snippet}
</Table>
