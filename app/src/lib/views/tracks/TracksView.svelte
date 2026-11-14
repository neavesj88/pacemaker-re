<script lang="ts">
  // The Tracks view: filter bar, the virtualised table, and the inspector on the right.
  import { tick, untrack } from "svelte";
  import { labelFilterActive } from "$lib/state/labels";
  import { filters } from "$lib/state/filters.svelte";
  import { nav } from "$lib/state/nav.svelte";
  import { suspects } from "$lib/state/gridSuspects.svelte";
  import { selection } from "$lib/state/selection.svelte";
  import { session } from "$lib/state/session.svelte";
  import { toast } from "$lib/state/toast.svelte";
  import FilterBar from "./FilterBar.svelte";
  import Inspector from "./Inspector.svelte";
  import TrackList from "./TrackList.svelte";
  import { tableRows } from "./tableRows.svelte";
  import { parseKey, uniqueGenres } from "./trackLogic";
  import { tracksState } from "./tracksState.svelte";

  const library = $derived(session.library);
  const all = $derived(tableRows.all);
  const genres = $derived(uniqueGenres(all));

  // header search + needs filter, then the toolbar's filters, then the sort (every key); the
  // player's next / previous walk the same list (trackFilter.tracksViewRows)
  const rows = $derived(tableRows.rows);

  const byId = $derived(new Map(all.map((t) => [t.track_id, t])));
  const focus = $derived(tracksState.focusId === null ? null : (byId.get(tracksState.focusId) ?? null));
  const filtersActive = $derived(
    !!(filters.query || filters.onlyNeeds || tracksState.genre || tracksState.bpmMin || tracksState.bpmMax || tracksState.keyFilter || tracksState.onlyAnalysed || labelFilterActive(tracksState.labelFilter) || tracksState.onlySuspect || tracksState.caseId !== null),
  );
  const suspectIds = $derived(tracksState.onlySuspect ? rows.map((t) => t.track_id) : []);

  // the filter needs a result even when checking on load is switched off, and again after a reload
  $effect(() => {
    if (tracksState.onlySuspect && library) void suspects.ensure();
  });

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

  // "Show tracks" in Library health (and any other list of tracks to show): select every one that
  // is in the library, focus the first and scroll to it
  $effect(() => {
    if (nav.params["tracks.trackIds"] === undefined || !library) return;
    untrack(() => {
      const ids = nav.take<number[]>("tracks.trackIds");
      if (Array.isArray(ids)) void showTracks(ids.filter((id) => typeof id === "number"));
    });
  });

  async function showTracks(want: number[]) {
    const present = want.filter((id) => byId.has(id));
    if (present.length === 0) {
      toast.show("None of those tracks is in the library any more.", "info");
      return;
    }
    const hiddenNow = () => {
      const seen = new Set(rows.map((r) => r.track_id));
      return present.filter((id) => !seen.has(id));
    };
    if (hiddenNow().length > 0 && tracksState.prefs.findClearsFilters) {
      tracksState.clearAllFilters();
      toast.show("Cleared the search and filters to show those tracks.", "info");
      await tick(); // the list is rebuilt without the filters
    }
    await tick(); // and the table exists
    const hidden = new Set(hiddenNow());
    const shown = rows.filter((r) => present.includes(r.track_id)).map((r) => r.track_id);
    if (shown.length === 0) {
      toast.show(`${present.length === 1 ? "That track is" : "Those tracks are"} hidden by the current search or filters. Clear them to see ${present.length === 1 ? "it" : "them"}.`, "info");
      return;
    }
    list?.reveal(shown[0], true);
    tracksState.selectedIds = shown; // reveal picks one; the rest of the list stays selected
    if (hidden.size > 0) toast.show(`${shown.length} of ${present.length} tracks shown; ${hidden.size} hidden by the current search or filters.`, "info");
    if (present.length < want.length) toast.show(`${want.length - present.length} of those tracks are no longer in the library.`, "info");
  }

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
  <FilterBar {genres} focusKey={focus ? parseKey(focus.key) : null} shown={rows.length} total={all.length} selected={selection.trackIds.length} {suspectIds} />
  <div class="split">
    <div class="main">
      {#if library}<TrackList bind:this={list} {rows} root={library.root} />{/if}
      {#if rows.length === 0}
        <p class="empty">
          {#if all.length === 0}
            No tracks {session.place.on} yet. Use Add tracks… or drop audio files here.
          {:else if filters.query}
            No tracks match the search. Clear the search box (or use Clear filters above) to see them all.
          {:else if tracksState.onlySuspect && suspects.checking}
            Checking the beat grids…
          {:else if tracksState.onlySuspect && suspects.error}
            Could not check the beat grids: {suspects.error}
          {:else if tracksState.onlySuspect && !filters.query && !tracksState.genre && !tracksState.bpmMin && !tracksState.bpmMax && !tracksState.keyFilter && !tracksState.onlyAnalysed && !filters.onlyNeeds}
            No suspect grids under the current rules. The Rules… button changes the genre windows and the confidence floor.
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
