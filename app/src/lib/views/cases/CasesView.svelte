<script lang="ts">
  import { caseTracks } from "$lib/api/cases";
  import type { Collection, Track } from "$lib/api/types";
  import { filters } from "$lib/state/filters.svelte";
  import { cases } from "$lib/state/cases.svelte";
  import { session } from "$lib/state/session.svelte";
  import Button from "$lib/ui/Button.svelte";
  import CollectionTable from "$lib/ui/CollectionTable.svelte";
  import TrackTable from "../tracks/TrackTable.svelte";
  import { visibleTracks } from "../tracks/trackFilter";

  async function openCase(c: Collection) {
    if (!session.library) return;
    try {
      cases.trackIds = await caseTracks(session.library.root, c.id);
      cases.open = c;
    } catch (e) {
      session.error = String(e);
    }
  }

  let rows = $derived.by(() => {
    if (!session.library) return [];
    const byId = new Map(session.library.tracks.map((t) => [t.track_id, t]));
    return visibleTracks(cases.trackIds.map((id) => byId.get(id)).filter((t): t is Track => !!t));
  });
  let total = $derived(cases.trackIds.length);
</script>

{#if cases.open}
  <div class="crumb">
    <Button small onclick={() => cases.close()}>← Cases</Button>
    <b>{cases.open.name}</b>
    <span>
      {total} track{total === 1 ? "" : "s"}{total > rows.length && !filters.query && !filters.onlyNeeds
        ? ` (${total - rows.length} no longer in the library)`
        : ""}
    </span>
  </div>
  <TrackTable {rows} />
  {#if rows.length === 0}
    <p class="empty">{filters.query || filters.onlyNeeds ? "No tracks match." : "This case is empty."}</p>
  {/if}
{:else}
  <CollectionTable rows={session.library?.cases ?? []} noun="cases" onopen={openCase} />
{/if}

<style>
  .crumb {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 10px;
    border-bottom: 1px solid var(--line);
  }
  .crumb span {
    color: var(--muted);
  }
</style>
