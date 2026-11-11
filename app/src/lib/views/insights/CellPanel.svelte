<script lang="ts">
  // The tracks of the picked cell, and the way into the Tracks view with the same ones filtered.
  import Button from "$lib/ui/Button.svelte";
  import Table, { type Column } from "$lib/ui/Table.svelte";
  import type { GapTrack } from "./gapLogic";
  import { energyOf } from "./gapLogic";

  let {
    title,
    tracks,
    canOpen,
    whyNot,
    showEnergy,
    onopen,
    onclose,
  }: {
    title: string;
    tracks: GapTrack[];
    /** The Tracks view can show exactly these tracks. */
    canOpen: boolean;
    /** Said when it cannot. */
    whyNot: string;
    showEnergy: boolean;
    onopen: () => void;
    onclose: () => void;
  } = $props();

  // A cell can hold a large share of a big library: list the first part, the Tracks view has all.
  const LIMIT = 200;

  const rows = $derived([...tracks].sort((a, b) => a.bpm - b.bpm || a.title.localeCompare(b.title) || a.track_id - b.track_id));
  const shown = $derived(rows.slice(0, LIMIT));
  const columns = $derived<Column[]>([
    { key: "title", label: "Title" },
    { key: "artist", label: "Artist" },
    { key: "bpm", label: "BPM", num: true },
    { key: "key", label: "Key" },
    ...(showEnergy ? [{ key: "energy", label: "Energy", num: true }] : []),
  ]);
</script>

<section class="panel" aria-label="Tracks in the picked cell">
  <div class="head">
    <h3>{title}</h3>
    <Button variant="ghost" small onclick={onclose} aria-label="Close the cell">Close</Button>
  </div>
  <p class="muted n">{tracks.length} track{tracks.length === 1 ? "" : "s"}</p>
  <div class="acts">
    <Button small variant="primary" disabled={!canOpen || tracks.length === 0} onclick={onopen}>Show these in Tracks</Button>
    {#if !canOpen}<span class="muted">{whyNot}</span>{/if}
  </div>
  {#if tracks.length === 0}
    <p class="empty">Nothing in this cell yet. That is the gap.</p>
  {:else}
    <div class="scroll">
      <Table {columns} rows={shown} rowKey={(t) => t.track_id}>
        {#snippet cell(t: GapTrack, c: Column)}
          {#if c.key === "title"}{t.title}
          {:else if c.key === "artist"}{t.artist}
          {:else if c.key === "bpm"}{t.bpm > 0 ? t.bpm.toFixed(1) : ""}
          {:else if c.key === "key"}{t.key}
          {:else}{energyOf(t) ?? ""}{/if}
        {/snippet}
      </Table>
    </div>
    {#if rows.length > shown.length}
      <p class="muted more">Showing the first {shown.length} by BPM. Show these in Tracks to see all {rows.length}.</p>
    {/if}
  {/if}
</section>

<style>
  .panel {
    border-bottom: 1px solid var(--line);
    padding: 10px 12px 12px;
  }
  .head {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    gap: 8px;
  }
  h3 {
    margin: 0;
    font-size: 14px;
    line-height: 1.3;
  }
  .n {
    margin: 2px 0 8px;
  }
  .acts {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
    margin-bottom: 8px;
  }
  .scroll {
    max-height: 340px;
    overflow: auto;
    border: 1px solid var(--line);
    border-radius: 4px;
  }
  .empty {
    padding: 8px 0;
  }
  .more {
    margin: 6px 0 0;
    font-size: 12px;
  }
</style>
