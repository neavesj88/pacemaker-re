<script lang="ts">
  // The Tracks view's table: virtualised (only the rows on screen exist, so 5000+ tracks scroll
  // smoothly), sortable, resizable columns, multi-select with mouse and keyboard, context menu.
  // `rows` must already be filtered and sorted (TracksView does that).
  import { onMount, tick } from "svelte";
  import type { Track } from "$lib/api/types";
  import { mmss } from "$lib/format";
  import { playback } from "$lib/state/playback.svelte";
  import { selection } from "$lib/state/selection.svelte";
  import { session } from "$lib/state/session.svelte";
  import ContextMenu from "$lib/ui/ContextMenu.svelte";
  import Stars from "$lib/ui/Stars.svelte";
  import type { ColDef } from "./columns";
  import MiniWave from "./MiniWave.svelte";
  import { addToCase } from "./addToCase.svelte";
  import { analysis } from "$lib/state/analysis.svelte";
  import { addToMix, buildSet, editGrid, editMetadata, playTrack, rate, revealInExplorer } from "./trackActions";
  import { keyColour, parseKey } from "./trackLogic";
  import { tracksState } from "./tracksState.svelte";

  let { rows, root }: { rows: Track[]; root: string } = $props();

  const ROW = 28;
  const HEAD = 30;
  const OVERSCAN = 6;

  let scroller = $state<HTMLDivElement | null>(null);
  let scrollTop = $state(0);
  let viewH = $state(600);
  let anchorId: number | null = null;

  const cols = $derived(tracksState.shownColumns);
  const widthOf = (c: ColDef) => tracksState.prefs.widths[c.key];
  const totalW = $derived(cols.reduce((sum, c) => sum + widthOf(c), 0));
  const template = $derived(cols.map((c) => `${widthOf(c)}px`).join(" "));
  const bodyH = $derived(Math.max(0, viewH - HEAD));
  const start = $derived(Math.max(0, Math.floor(scrollTop / ROW) - OVERSCAN));
  const end = $derived(Math.min(rows.length, Math.ceil((scrollTop + bodyH) / ROW) + OVERSCAN));
  const slice = $derived(rows.slice(start, end));
  const selSet = $derived(new Set(selection.trackIds));

  // ---- selection ------------------------------------------------------------------------

  const indexOfId = (id: number | null) => (id === null ? -1 : rows.findIndex((r) => r.track_id === id));

  function select(ids: number[], focus: number | null) {
    selection.trackIds = ids;
    tracksState.focusId = focus;
  }

  function rangeIds(fromId: number | null, toIndex: number): number[] {
    const a = indexOfId(fromId);
    const from = a < 0 ? toIndex : a;
    const [lo, hi] = from <= toIndex ? [from, toIndex] : [toIndex, from];
    return rows.slice(lo, hi + 1).map((r) => r.track_id);
  }

  function clickRow(e: MouseEvent, t: Track, index: number) {
    const id = t.track_id;
    if (e.shiftKey) {
      const range = rangeIds(anchorId, index);
      select(e.ctrlKey || e.metaKey ? [...new Set([...selection.trackIds, ...range])] : range, id);
    } else if (e.ctrlKey || e.metaKey) {
      anchorId = id;
      if (selSet.has(id)) {
        const rest = selection.trackIds.filter((x) => x !== id);
        select(rest, rest.at(-1) ?? null);
      } else {
        select([...selection.trackIds, id], id);
      }
    } else {
      anchorId = id;
      select([id], id);
    }
  }

  /** Select just this track and bring it into view; false when the filters hide it. */
  export function reveal(id: number): boolean {
    const i = indexOfId(id);
    if (i < 0) return false;
    anchorId = id;
    select([id], id);
    scrollToIndex(i);
    return true;
  }

  function scrollToIndex(i: number) {
    if (!scroller) return;
    const top = i * ROW;
    if (top < scroller.scrollTop) scroller.scrollTop = top;
    else if (top + ROW > scroller.scrollTop + bodyH) scroller.scrollTop = top + ROW - bodyH;
  }

  function moveTo(index: number, extend: boolean) {
    if (rows.length === 0) return;
    const i = Math.max(0, Math.min(rows.length - 1, index));
    const id = rows[i].track_id;
    if (extend) {
      if (anchorId === null) anchorId = tracksState.focusId ?? id;
      select(rangeIds(anchorId, i), id);
    } else {
      anchorId = id;
      select([id], id);
    }
    scrollToIndex(i);
  }

  function onKey(e: KeyboardEvent) {
    if (e.altKey) return;
    // Keys pressed on the heart, the stars or a column header belong to that control.
    if (e.target instanceof Element && e.target.closest("button, input, select, textarea, .hcell")) return;
    const cursor = indexOfId(tracksState.focusId);
    const page = Math.max(1, Math.floor(bodyH / ROW) - 1);
    const mod = e.ctrlKey || e.metaKey;
    switch (e.key) {
      case "ArrowDown": moveTo(cursor + 1, e.shiftKey); break;
      case "ArrowUp": moveTo(cursor < 0 ? 0 : cursor - 1, e.shiftKey); break;
      case "PageDown": moveTo(cursor + page, e.shiftKey); break;
      case "PageUp": moveTo(cursor - page, e.shiftKey); break;
      case "Home": moveTo(0, e.shiftKey); break;
      case "End": moveTo(rows.length - 1, e.shiftKey); break;
      case "a":
        if (!mod) return;
        select(rows.map((r) => r.track_id), tracksState.focusId ?? rows[0]?.track_id ?? null);
        break;
      case "Escape": select([], null); break;
      case "Enter":
        if (tracksState.focusId === null) return;
        playTrack(tracksState.focusId);
        break;
      case " ":
        if (tracksState.focusId === null) return;
        if (playback.trackId === tracksState.focusId) playback.toggle();
        else playTrack(tracksState.focusId);
        break;
      default: {
        // 0-5 rates the track the inspector shows
        const stars = e.key.length === 1 && !mod ? "012345".indexOf(e.key) : -1;
        const t = stars >= 0 ? rows[cursor] : undefined;
        if (!t) return;
        rate(t, stars);
      }
    }
    e.preventDefault();
  }

  // Filters can hide selected tracks; nothing hidden stays selected (a menu action must never
  // touch a track the user can't see).
  $effect(() => {
    const visible = new Set(rows.map((r) => r.track_id));
    const kept = selection.trackIds.filter((id) => visible.has(id));
    if (kept.length !== selection.trackIds.length) selection.trackIds = kept;
    if (tracksState.focusId !== null && !visible.has(tracksState.focusId)) tracksState.focusId = kept.at(-1) ?? null;
  });

  onMount(async () => {
    await tick();
    const i = indexOfId(tracksState.focusId);
    if (i >= 0) scrollToIndex(i);
  });

  // ---- column resize --------------------------------------------------------------------

  function startResize(e: PointerEvent, c: ColDef) {
    e.preventDefault();
    e.stopPropagation();
    const handle = e.currentTarget as HTMLElement;
    handle.setPointerCapture(e.pointerId);
    const x0 = e.clientX;
    const w0 = widthOf(c);
    const move = (m: PointerEvent) => tracksState.setWidth(c.key, w0 + m.clientX - x0);
    const stop = () => {
      handle.removeEventListener("pointermove", move);
      handle.removeEventListener("pointerup", stop);
      handle.removeEventListener("pointercancel", stop);
      tracksState.save();
    };
    handle.addEventListener("pointermove", move);
    handle.addEventListener("pointerup", stop);
    handle.addEventListener("pointercancel", stop);
  }

  // ---- context menu ---------------------------------------------------------------------

  let menu = $state<{ x: number; y: number; ids: number[] } | null>(null);

  function openMenu(e: MouseEvent, t: Track) {
    e.preventDefault();
    e.stopPropagation(); // the menu closes on window events; this one must not reach it
    if (!selSet.has(t.track_id)) {
      anchorId = t.track_id;
      select([t.track_id], t.track_id);
    }
    menu = { x: Math.max(0, Math.min(e.clientX, innerWidth - 230)), y: Math.max(0, Math.min(e.clientY, innerHeight - 250)), ids: [...selection.trackIds] };
  }

  const menuItems = $derived.by(() => {
    if (!menu) return [];
    const ids = menu.ids;
    const one = ids.length === 1;
    const n = ids.length;
    const allFav = ids.every((id) => tracksState.favourites.has(id));
    return [
      { label: "Play", onclick: () => playTrack(ids[0]) },
      { label: one ? "Edit metadata…" : `Edit metadata of ${n} tracks…`, onclick: () => editMetadata(ids) },
      { label: one ? "Add to mix" : `Add ${n} tracks to mix`, onclick: () => addToMix(ids) },
      { label: one ? "Add to case…" : `Add ${n} tracks to case…`, onclick: () => addToCase.ask(ids) },
      { label: one ? "Build a set…" : `Build a set from ${n} tracks…`, onclick: () => buildSet(ids), disabled: one },
      {
        label: allFav ? (one ? "Remove from favourites" : `Remove ${n} from favourites`) : one ? "Favourite" : `Favourite ${n} tracks`,
        onclick: () => tracksState.setFavourite(ids, !allFav),
      },
      { label: one ? "Detect key" : `Detect keys of ${n} tracks`, onclick: () => analysis.askDetectKeys(ids), disabled: !ids.some((id) => analysis.keyless.some((t) => t.track_id === id)) },
      { label: "Edit grid", onclick: () => editGrid(ids[0]), disabled: !one },
      { label: "Show in Explorer", onclick: () => revealInExplorer(ids[0]), disabled: !one },
    ];
  });

  const stop = (e: Event) => e.stopPropagation();
</script>

<!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
<div
  class="scroller"
  role="grid"
  aria-label="Tracks"
  aria-multiselectable="true"
  aria-rowcount={rows.length}
  tabindex="0"
  bind:this={scroller}
  bind:clientHeight={viewH}
  onscroll={(e) => (scrollTop = e.currentTarget.scrollTop)}
  onkeydown={onKey}
>
  <div class="head" role="row" style:grid-template-columns={template} style:min-width="{totalW}px" style:height="{HEAD}px">
    {#each cols as c (c.key)}
      <div
        class="hcell"
        class:num={c.num}
        class:sortable={!!c.sort}
        role="columnheader"
        tabindex={c.sort ? 0 : -1}
        aria-sort={c.sort && c.sort === tracksState.prefs.sortKey ? (tracksState.prefs.sortAsc ? "ascending" : "descending") : "none"}
        onclick={() => c.sort && tracksState.sortBy(c.sort)}
        onkeydown={(e) => {
          if (e.key !== "Enter" && e.key !== " ") return;
          e.stopPropagation();
          if (!c.sort) return;
          e.preventDefault();
          tracksState.sortBy(c.sort);
        }}
      >
        {#if c.key === "fav"}<span class="heart-h" title="Favourites">♥</span>{:else}{c.label}{/if}
        {#if c.sort && c.sort === tracksState.prefs.sortKey}<span class="arrow">{tracksState.prefs.sortAsc ? "▲" : "▼"}</span>{/if}
        <!-- svelte-ignore a11y_click_events_have_key_events -->
        <span class="grip" role="presentation" onpointerdown={(e) => startResize(e, c)} onclick={stop} title="Drag to resize"></span>
      </div>
    {/each}
  </div>

  <div class="body" style:height="{rows.length * ROW}px" style:min-width="{totalW}px">
    {#each slice as t, k (t.track_id)}
      {@const i = start + k}
      {@const need = session.needs.has(t.track_id)}
      {@const fav = tracksState.favourites.has(t.track_id)}
      {@const selected = selSet.has(t.track_id)}
      <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
      <div
        class="row"
        class:alt={i % 2 === 1}
        class:sel={selected}
        class:dim={need}
        class:focus={t.track_id === tracksState.focusId}
        role="row"
        tabindex="-1"
        aria-selected={selected}
        aria-rowindex={i + 1}
        style:top="{i * ROW}px"
        style:height="{ROW}px"
        style:grid-template-columns={template}
        onclick={(e) => clickRow(e, t, i)}
        ondblclick={() => playTrack(t.track_id)}
        oncontextmenu={(e) => openMenu(e, t)}
      >
        {#each cols as c (c.key)}
          <div class="cell" class:num={c.num} role="gridcell">
            {#if c.key === "fav"}
              <button
                class="heart"
                class:on={fav}
                aria-label={fav ? "Remove from favourites" : "Add to favourites"}
                aria-pressed={fav}
                onclick={(e) => (stop(e), tracksState.setFavourite([t.track_id], !fav))}
                ondblclick={stop}
              >♥</button>
            {:else if c.key === "title"}
              {#if playback.trackId === t.track_id}<span class="now" title="Loaded in the player">▶</span>{/if}
              <span class="text">{t.title}</span>
              {#if need}<span class="badge" title={session.needs.get(t.track_id)}>needs analysis</span>{/if}
            {:else if c.key === "artist"}
              <span class="text">{t.artist}</span>
            {:else if c.key === "album"}
              <span class="text">{t.album}</span>
            {:else if c.key === "genre"}
              <span class="text">{t.genre}</span>
            {:else if c.key === "wave"}
              {#if t.location}<MiniWave {root} location={t.location} />{/if}
            {:else if c.key === "bpm"}
              {t.bpm && !need ? t.bpm.toFixed(2) : "–"}
            {:else if c.key === "key"}
              {@const k = parseKey(t.key)}
              {#if k}
                <span class="key" style:color={keyColour(k)} style:background="color-mix(in srgb, {keyColour(k)} 14%, transparent)" title={t.key}>{k.code}</span>
              {/if}
            {:else if c.key === "rating"}
              <span class="rate" role="presentation" onclick={stop} ondblclick={stop}>
                <Stars rating={t.rating} onchange={(n) => rate(t, n)} />
              </span>
            {:else if c.key === "plays"}
              {t.times_played || ""}
            {:else if c.key === "time"}
              {mmss(t.play_time_secs)}
            {/if}
          </div>
        {/each}
      </div>
    {/each}
  </div>
</div>

{#if menu}
  <ContextMenu x={menu.x} y={menu.y} items={menuItems} onclose={() => (menu = null)} />
{/if}

<style>
  .scroller {
    position: relative;
    flex: 1;
    min-width: 0;
    min-height: 0;
    overflow: auto;
    outline: none;
  }
  .scroller:focus-visible {
    box-shadow: inset 0 0 0 1px var(--line-hi);
  }
  .head {
    position: sticky;
    top: 0;
    z-index: 2;
    display: grid;
    background: var(--deep);
    border-bottom: 1px solid var(--line);
    box-sizing: border-box;
  }
  .hcell {
    position: relative;
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 0 10px;
    color: var(--muted);
    font-weight: 600;
    white-space: nowrap;
    overflow: hidden;
    user-select: none;
  }
  .hcell.sortable {
    cursor: pointer;
  }
  .hcell.sortable:hover {
    color: var(--text);
  }
  .hcell.num {
    justify-content: flex-end;
  }
  .arrow {
    font-size: 9px;
    color: var(--accent);
  }
  .heart-h {
    color: var(--muted);
  }
  .grip {
    position: absolute;
    top: 0;
    right: 0;
    width: 7px;
    height: 100%;
    cursor: col-resize;
    touch-action: none;
  }
  .grip:hover {
    background: var(--line-hi);
  }
  .body {
    position: relative;
  }
  .row {
    position: absolute;
    left: 0;
    right: 0;
    display: grid;
    align-items: center;
    background: var(--row);
    box-sizing: border-box;
    cursor: default;
    outline: none;
  }
  .row.alt {
    background: var(--row-alt);
  }
  .row:hover {
    background: var(--hover);
  }
  .row.sel {
    background: color-mix(in srgb, var(--accent) 20%, var(--row));
  }
  .row.focus {
    box-shadow: inset 2px 0 0 var(--accent);
  }
  .row.dim {
    color: var(--muted);
  }
  .cell {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
    padding: 0 10px;
    white-space: nowrap;
    overflow: hidden;
  }
  .cell.num {
    justify-content: flex-end;
    font-variant-numeric: tabular-nums;
  }
  .text {
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .now {
    color: var(--accent);
    font-size: 10px;
  }
  .badge {
    flex: none;
    padding: 0 6px;
    border-radius: 8px;
    font-size: 11px;
    background: var(--accent-soft);
    color: var(--accent);
  }
  .heart {
    width: 100%;
    background: none;
    border: none;
    padding: 0;
    color: var(--line-hi);
    font: inherit;
    cursor: pointer;
  }
  .heart:hover {
    color: var(--muted);
  }
  .heart.on {
    color: var(--accent);
  }
  .key {
    padding: 0 6px;
    border-radius: 3px;
    font-size: 12px;
    font-variant-numeric: tabular-nums;
  }
  .rate {
    display: inline-flex;
  }
</style>
