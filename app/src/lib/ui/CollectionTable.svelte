<script lang="ts">
  import type { Collection } from "$lib/api/types";
  import { mmss } from "$lib/format";
  import Button from "./Button.svelte";
  import Table from "./Table.svelte";

  // Name / tracks / length list used for mixes and cases. Pass onopen to make rows clickable.
  let {
    rows,
    noun,
    onopen,
  }: { rows: Collection[]; noun: string; onopen?: (c: Collection) => void } = $props();

  const columns = [
    { key: "name", label: "Name" },
    { key: "track_count", label: "Tracks", num: true },
    { key: "play_time_secs", label: "Length", num: true },
  ];
</script>

<Table {columns} {rows} rowKey={(c) => c.id} onrowclick={onopen}>
  {#snippet cell(c, col)}
    {#if col.key === "name"}
      {#if onopen}
        <Button variant="link" onclick={(e) => (e.stopPropagation(), onopen(c))}>{c.name}</Button>
      {:else}
        {c.name}
      {/if}
    {:else if col.key === "track_count"}
      {c.track_count}
    {:else}
      {mmss(c.play_time_secs)}
    {/if}
  {/snippet}
</Table>
{#if rows.length === 0}
  <p class="empty">No {noun} on this device yet.</p>
{/if}
