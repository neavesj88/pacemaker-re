<script lang="ts">
  // The Tracks view: filter bar, the virtualised table, and the inspector on the right.
  import { filters } from "$lib/state/filters.svelte";
  import { selection } from "$lib/state/selection.svelte";
  import { session } from "$lib/state/session.svelte";
  import { toast } from "$lib/state/toast.svelte";
  import FilterBar from "./FilterBar.svelte";
  import Inspector from "./Inspector.svelte";
  import TrackList from "./TrackList.svelte";
  import { searchTracks } from "./trackFilter";
  import { filterTracks, parseKey, sortTracks, uniqueGenres } from "./trackLogic";
  import { tracksState } from "./tracksState.svelte";

  const library = $derived(session.library);
  const all = $derived(library?.tracks ?? []);
  const genres = $derived(uniqueGenres(all));

  // header search + needs filter, then the toolbar's filters, then the sort
  const rows = $derived(
    sortTracks(
      filterTracks(searchTracks(all), tracksState.filter, session.needs),
      tracksState.prefs.sortKey,
      tracksState.prefs.sortAsc,
      (id) => tracksState.favourites.has(id),
    ),
  );

  const byId = $derived(new Map(all.map((t) => [t.track_id, t])));
  const focus = $derived(tracksState.focusId === null ? null : (byId.get(tracksState.focusId) ?? null));
  const filtersActive = $derived(
    !!(filters.query || filters.onlyNeeds || tracksState.genre || tracksState.bpmMin || tracksState.bpmMax || tracksState.keyFilter || tracksState.onlyAnalysed),
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
            No tracks match the search.
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
  }
</style>
