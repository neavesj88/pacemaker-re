<script lang="ts">
  // The track picker of the Mix tab: the real Tracks table (every column, sorting, the filter bar
  // with the case filter and label filter, the query search) with its own sort, columns, filters
  // and selection, remembered under its own key. Drag rows onto the mix bar (pointer drag), or
  // double-click / Enter a row, or use the buttons. The panel is resized by dragging its top edge.
  import { onDestroy, onMount } from "svelte";
  import type { Track } from "$lib/api/types";
  import { Filters } from "$lib/state/filters.svelte";
  import { session } from "$lib/state/session.svelte";
  import { endTrackDrag } from "$lib/state/trackDrag.svelte";
  import { mixes } from "$lib/state/mixes.svelte";
  import FilterBar from "../tracks/FilterBar.svelte";
  import TrackList from "../tracks/TrackList.svelte";
  import { MIX_PICKER_KEY, mixPickerDefaults } from "../tracks/columns";
  import { TableState } from "../tracks/tableState.svelte";
  import { tracksViewRows } from "../tracks/trackFilter";
  import { parseKey, uniqueGenres } from "../tracks/trackLogic";
  import { tracksState } from "../tracks/tracksState.svelte";
  import { addTracks } from "./actions";
  import { PICKER_MIN, mv } from "./view.svelte";

  onDestroy(endTrackDrag);

  // This table's own search box, selection and layout; the favourites are the library's.
  const search = new Filters();
  const selection = $state({ trackIds: [] as number[] });
  const table = new TableState({ storageKey: MIX_PICKER_KEY, defaults: mixPickerDefaults, search, selection, favourites: tracksState.favourites });

  const library = $derived(session.library);
  const all = $derived(library?.tracks ?? []);
  const genres = $derived(uniqueGenres(all));
  const rows = $derived(tracksViewRows(all, table));
  const byId = $derived(new Map(all.map((t) => [t.track_id, t])));
  const focus = $derived(table.focusId === null ? null : (byId.get(table.focusId) ?? null));

  // the hearts of the table: read the library's favourites even if the Tracks view was never opened
  onMount(() => {
    const root = library?.root;
    if (root && tracksState.favourites.size === 0) void tracksState.loadFavourites(root);
  });

  // a genre this library does not have would silently hide everything
  $effect(() => {
    if (table.genre && !genres.includes(table.genre)) table.genre = "";
  });

  /** How many times each track is already in the mix. */
  const used = $derived.by(() => {
    const m = new Map<number, number>();
    for (const c of mixes.doc?.clips ?? []) m.set(c.trackId, (m.get(c.trackId) ?? 0) + 1);
    return m;
  });
  const marker = (id: number) => {
    const n = used.get(id) ?? 0;
    return n > 0 ? `×${n}` : "";
  };

  const chosen = $derived(selection.trackIds.filter((id) => byId.has(id)));

  function addSelected(at?: number) {
    if (chosen.length === 0) return;
    void addTracks(chosen, at);
  }

  function open(t: Track) {
    void addTracks([t.track_id], mv.playhead);
  }

  /** Dragging the top edge: the table grows upwards, so the mix above it shrinks. */
  function resize(e: PointerEvent) {
    if (e.button !== 0) return;
    e.preventDefault();
    const el = e.currentTarget as HTMLElement;
    el.setPointerCapture(e.pointerId);
    const y0 = e.clientY;
    const h0 = mv.pickerH;
    const max = () => Math.max(PICKER_MIN, innerHeight - 260);
    const move = (m: PointerEvent) => mv.setPickerHeight(Math.min(max(), h0 + (y0 - m.clientY)));
    const up = () => {
      el.removeEventListener("pointermove", move);
      el.removeEventListener("pointerup", up);
      el.removeEventListener("pointercancel", up);
      mv.setPickerHeight(mv.pickerH, true);
    };
    el.addEventListener("pointermove", move);
    el.addEventListener("pointerup", up);
    el.addEventListener("pointercancel", up);
  }

  function gripKey(e: KeyboardEvent) {
    if (e.key !== "ArrowUp" && e.key !== "ArrowDown") return;
    e.preventDefault();
    e.stopPropagation();
    mv.setPickerHeight(mv.pickerH + (e.key === "ArrowUp" ? 40 : -40), true);
  }
</script>

<section class="picker" style:height="{mv.pickerH}px" aria-label="Library tracks">
  <!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
  <div class="grip" role="separator" aria-orientation="horizontal" aria-label="Resize the track table" tabindex="0" title="Drag to resize the track table (Up / Down arrow keys too)" onpointerdown={resize} onkeydown={gripKey}>
    <i></i>
  </div>
  <div class="bar">
    <FilterBar {table} ownSearch withPresets={false} {genres} focusKey={focus ? parseKey(focus.key) : null} shown={rows.length} total={all.length} selected={chosen.length} />
  </div>
  <div class="tbl">
    {#if library}
      <TrackList {rows} root={library.root} {table} label="Library tracks" dragFrom="mix:picker" onopen={open} onadd={(ids) => void addTracks(ids, mv.playhead)} {marker} />
    {/if}
    {#if rows.length === 0}
      <p class="empty">{all.length === 0 ? "No tracks in this library yet." : "No tracks match. Clear the search or the filters above."}</p>
    {/if}
  </div>
  <div class="foot">
    <button class="primary" disabled={chosen.length === 0} onclick={() => addSelected(mv.playhead)}>Add at playhead</button>
    <button disabled={chosen.length === 0} onclick={() => addSelected()}>Add to end</button>
    <span class="muted">{chosen.length > 0 ? `${chosen.length} selected` : "Select tracks, then add them"} · drag rows onto the bar · double-click or Enter adds at the playhead</span>
  </div>
</section>

<style>
  .picker {
    position: relative;
    display: flex;
    flex-direction: column;
    flex: 0 1 auto;
    min-height: 160px;
    min-width: 0;
    border-top: 1px solid var(--line-hi);
    background: var(--panel);
  }
  .grip {
    position: absolute;
    left: 0;
    right: 0;
    top: -4px;
    height: 9px;
    z-index: 5;
    cursor: ns-resize;
    display: flex;
    align-items: center;
    justify-content: center;
    outline: none;
  }
  .grip i {
    width: 44px;
    height: 3px;
    border-radius: 2px;
    background: var(--line-hi);
  }
  .grip:hover i,
  .grip:focus-visible i {
    background: var(--accent);
  }
  .bar {
    flex: none;
  }
  .tbl {
    position: relative;
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }
  .empty {
    position: absolute;
    left: 0;
    right: 0;
    top: 70px;
    margin: 0;
    text-align: center;
    color: var(--muted);
    pointer-events: none;
  }
  .foot {
    flex: none;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 12px;
    border-top: 1px solid var(--line);
    background: var(--deep);
  }
  .muted {
    color: var(--muted);
    font-size: 12px;
  }
</style>
