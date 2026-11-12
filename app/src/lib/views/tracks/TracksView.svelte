<script lang="ts">
  // The Tracks view: filter bar, the virtualised table, and the inspector on the right.
  import { tick, untrack } from "svelte";
  import { labelFilterActive } from "$lib/state/labels";
  import { filters } from "$lib/state/filters.svelte";
  import { nav } from "$lib/state/nav.svelte";
  import { selection } from "$lib/state/selection.svelte";
  import { session } from "$lib/state/session.svelte";
  import { toast } from "$lib/state/toast.svelte";
  import FilterBar from "./FilterBar.svelte";
  import Inspector from "./Inspector.svelte";
  import TrackList from "./TrackList.svelte";
  import { tracksViewRows } from "./trackFilter";
  import { parseKey, uniqueGenres } from "./trackLogic";
  import { tracksState } from "./tracksState.svelte";

  const library = $derived(session.library);
  const all = $derived(library?.tracks ?? []);
  const genres = $derived(uniqueGenres(all));

  // header search + needs filter, then the toolbar's filters, then the sort (every key); the
  // player's next / previous walk the same list (trackFilter.tracksViewRows)
  const rows = $derived(tracksViewRows(all));

  const byId = $derived(new Map(all.map((t) => [t.track_id, t])));
  const focus = $derived(tracksState.focusId === null ? null : (byId.get(tracksState.focusId) ?? null));
  const filtersActive = $derived(
    !!(filters.query || filters.onlyNeeds || tracksState.genre || tracksState.bpmMin || tracksState.bpmMax || tracksState.keyFilter || tracksState.onlyAnalysed || labelFilterActive(tracksState.labelFilter)),
  );

  // favourites belong to the open library; read them when it (or a different one) opens
  $effect(() => {
    const root = library?.root;
    if (root) tracksState.loadFavourites(root);
  });

  // a genre that the opened library doesn't have would silently hide everything
  $effect(() => {
    if (tracksState.genre && !genres.includes(tracksState.genre)) tracksState.genre = "";
  });

  let list = $state<ReturnType<typeof TrackList> | null>(null);

  function pick(id: number) {
    if (!list?.reveal(id)) toast.show("That track is hidden by the current search or filters.", "info");
  }

  // "Find" in History (and any other deep link) opens this view with a track to show
  $effect(() => {
    if (nav.params["tracks.trackId"] === undefined || !library) return;
    untrack(() => {
      const id = nav.take<number>("tracks.trackId");
      if (typeof id === "number") void findTrack(id);
    });
  });

  async function findTrack(id: number) {
    if (!byId.has(id)) {
      toast.show("That track is no longer in the library.", "info");
      return;
    }
    if (!rows.some((r) => r.track_id === id)) {
      if (!tracksState.prefs.findClearsFilters) {
        toast.show("That track is hidden by the current search or filters. Clear them to see it.", "info");
        return;
      }
      tracksState.clearAllFilters();
      toast.show("Cleared the search and filters to show that track.", "info");
    }
    await tick(); // the list is rebuilt without the filters
    await tick(); // and the table exists
    if (!list?.reveal(id, true)) toast.show("That track is hidden by the current search or filters.", "info");
  }
</script>

<div class="tracks">
  <FilterBar {genres} focusKey={focus ? parseKey(focus.key) : null} shown={rows.length} total={all.length} selected={selection.trackIds.length} />
  <div class="split">
    <div class="main">
      {#if library}<TrackList bind:this={list} {rows} root={library.root} />{/if}
      {#if rows.length === 0}
        <p class="empty">
          {#if all.length === 0}
            No tracks on this device yet. Use Add tracks… or drop audio files here.
          {:else if filters.query}
            No tracks match the search. Clear the search box (or use Clear filters above) to see them all.
          {:else if filtersActive}
            No tracks match the filters. Use Clear filters above to see them all.
          {/if}
        </p>
      {/if}
    </div>
    {#if library}
      <Inspector track={focus} selectedIds={selection.trackIds} root={library.root} onpick={pick} />
    {/if}
  </div>
</div>

<style>
  .tracks {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
  }
  .split {
    display: flex;
    flex: 1;
    min-height: 0;
  }
  .main {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-width: 0;
    min-height: 0;
    position: relative;
  }
  /* sits just under the header row instead of at the bottom of the empty grid */
  .empty {
    position: absolute;
    top: 40px;
    left: 0;
    right: 0;
    margin: 0;
    padding: 16px;
    color: var(--muted);
    pointer-events: none;
  }
</style>
