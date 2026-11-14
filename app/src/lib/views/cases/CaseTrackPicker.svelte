<script lang="ts">
  // Picks library tracks to add to a case. It is the real Tracks table (every column, multi-key
  // sorting, the filter bar with search, case and label filters) with its own layout, filters and
  // selection, like the Mix tab's picker. Tracks the case already holds are marked and skipped.
  import { onMount } from "svelte";
  import type { Track } from "$lib/api/types";
  import { Filters } from "$lib/state/filters.svelte";
  import { session } from "$lib/state/session.svelte";
  import { endTrackDrag } from "$lib/state/trackDrag.svelte";
  import { toast } from "$lib/state/toast.svelte";
  import Button from "$lib/ui/Button.svelte";
  import FilterBar from "../tracks/FilterBar.svelte";
  import TrackList from "../tracks/TrackList.svelte";
  import { TableState } from "../tracks/tableState.svelte";
  import { tracksViewRows } from "../tracks/trackFilter";
  import { parseKey, uniqueGenres } from "../tracks/trackLogic";
  import { tracksState } from "../tracks/tracksState.svelte";
  import { addSummary } from "./caseLogic";
  import { CASE_PICKER_KEY, casePickerDefaults } from "./casePicker";

  let {
    caseName,
    existing,
    onadd,
    onclose,
  }: { caseName: string; existing: Set<number>; onadd: (ids: number[]) => void | Promise<void>; onclose: () => void } = $props();

  // This table's own search box, selection and layout; the favourites are the library's.
  const search = new Filters();
  const selection = $state({ trackIds: [] as number[] });
  const table = new TableState({ storageKey: CASE_PICKER_KEY, defaults: casePickerDefaults, search, selection, favourites: tracksState.favourites });

  const library = $derived(session.library);
  const all = $derived(library?.tracks ?? []);
  const genres = $derived(uniqueGenres(all));
  const rows = $derived(tracksViewRows(all, table));
  const byId = $derived(new Map(all.map((t) => [t.track_id, t])));
  const focus = $derived(table.focusId === null ? null : (byId.get(table.focusId) ?? null));

  let panel = $state<HTMLDivElement | null>(null);

  // Escape closes the picker only when nothing opened from it (the metadata editor, a menu) is on top
  function escape(e: KeyboardEvent) {
    if (e.key !== "Escape" || e.defaultPrevented) return;
    const dialogs = document.querySelectorAll('[role="dialog"]');
    if (dialogs.length > 0 && dialogs[dialogs.length - 1] !== panel) return;
    onclose();
  }

  onMount(() => {
    panel?.focus();
    const root = library?.root;
    if (root && tracksState.favourites.size === 0) void tracksState.loadFavourites(root);
    return endTrackDrag;
  });

  // a genre this library does not have would silently hide everything
  $effect(() => {
    if (table.genre && !genres.includes(table.genre)) table.genre = "";
  });

  const chosen = $derived(selection.trackIds.filter((id) => byId.has(id)));
  const label = (t: Track) => (t.artist ? `${t.artist} – ${t.title}` : t.title);
  const plan = $derived(addSummary(chosen.map((id) => ({ id, label: label(byId.get(id)!) })), existing));
  const noun = (n: number) => `${n} track${n === 1 ? "" : "s"}`;

  async function add(ids: number[], close: boolean) {
    if (ids.length === 0) return;
    await onadd(ids);
    if (close) onclose();
  }

  /** Double-click / Enter on a row adds that track and keeps the picker open for more. */
  function open(t: Track) {
    if (existing.has(t.track_id)) {
      toast.show(`“${t.title}” is already in “${caseName}”`, "ok");
      return;
    }
    void add([t.track_id], false);
  }
</script>

<svelte:window onkeydown={escape} />

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="scrim" onclick={onclose}>
  <div class="panel" role="dialog" aria-modal="true" tabindex="-1" bind:this={panel} aria-label={`Add tracks to ${caseName}`} onclick={(e) => e.stopPropagation()}>
    <h2>Add tracks to “{caseName}”</h2>
    <div class="bar">
      <FilterBar {table} ownSearch withPresets={false} {genres} focusKey={focus ? parseKey(focus.key) : null} shown={rows.length} total={all.length} selected={chosen.length} />
    </div>
    <div class="tbl">
      {#if library}
        <TrackList {rows} root={library.root} {table} label="Library tracks" dragFrom="picker:case" onopen={open} marker={(id) => (existing.has(id) ? "in case" : "")} />
      {/if}
      {#if rows.length === 0}
        <p class="empty">{all.length === 0 ? "No tracks in this library yet." : "No tracks match. Clear the search or the filters above."}</p>
      {/if}
    </div>
    <div class="foot">
      <div class="what" aria-live="polite">
        {#if chosen.length === 0}
          <span class="muted">Select tracks, then add them. Double-click or Enter adds one at once. Tracks already in the case are marked and skipped.</span>
        {:else if plan.fresh.length === 0}
          <span class="muted">All {noun(chosen.length)} selected {chosen.length === 1 ? "is" : "are"} already in “{caseName}”.</span>
        {:else}
          <span>Adding {noun(plan.fresh.length)} to “{caseName}”: {plan.list}{plan.skipped > 0 ? ` (${plan.skipped} already there, skipped)` : ""}</span>
        {/if}
      </div>
      <Button onclick={onclose}>Done</Button>
      <Button variant="primary" disabled={plan.fresh.length === 0} onclick={() => add(plan.fresh, true)}>{plan.fresh.length === 0 ? "Add" : `Add ${noun(plan.fresh.length)}`}</Button>
    </div>
  </div>
</div>

<style>
  .scrim {
    position: fixed;
    inset: 0;
    z-index: 30;
    display: flex;
    align-items: center;
    justify-content: center;
    background: var(--scrim);
  }
  .panel {
    display: flex;
    flex-direction: column;
    width: min(1400px, 96vw);
    height: min(820px, 92vh);
    min-width: 0;
    background: var(--panel);
    border: 1px solid var(--line-hi);
    border-radius: 8px;
    overflow: hidden;
  }
  h2 {
    flex: none;
    margin: 0;
    padding: 12px 16px 8px;
    font-size: 16px;
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
    padding: 8px 16px;
    border-top: 1px solid var(--line);
    background: var(--deep);
  }
  .what {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 12px;
  }
  .muted {
    color: var(--muted);
  }
</style>
