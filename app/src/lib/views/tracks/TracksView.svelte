<script lang="ts">
  import { filters } from "$lib/state/filters.svelte";
  import { session } from "$lib/state/session.svelte";
  import TrackTable from "./TrackTable.svelte";
  import { visibleTracks } from "./trackFilter";

  let rows = $derived(session.library ? visibleTracks(session.library.tracks) : []);
</script>

<TrackTable {rows} />
{#if rows.length === 0}
  <p class="empty">
    {filters.query
      ? "No tracks match the search."
      : filters.onlyNeeds
        ? "Nothing needs analysis."
        : "No tracks on this device yet. Use Add tracks… or drop audio files here."}
  </p>
{/if}
