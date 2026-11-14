<script lang="ts">
  // A track's BPM or key as the Tracks table shows them (two decimals, a dash for a track that
  // needs analysis; the key as its Camelot code with the stored name on hover), so a track reads
  // the same in History and the play log as everywhere else.
  import type { Track } from "$lib/api/types";
  import { session } from "$lib/state/session.svelte";
  import { bpmText, keyColour, parseKey } from "../tracks/trackLogic";

  let { track, field }: { track: Track; field: "bpm" | "key" } = $props();

  const key = $derived(field === "key" ? parseKey(track.key) : null);
</script>

{#if field === "bpm"}
  {bpmText(track.bpm, session.needs.has(track.track_id))}
{:else if key}
  <span class="key" style:color={keyColour(key)} style:background="color-mix(in srgb, {keyColour(key)} 14%, transparent)" title={track.key}>{key.code}</span>
{/if}

<style>
  .key {
    padding: 0 6px;
    border-radius: 3px;
    font-size: 12px;
    font-variant-numeric: tabular-nums;
  }
</style>
