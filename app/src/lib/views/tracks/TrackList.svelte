<script lang="ts">
  // The Tracks view's table: virtualised (only the rows on screen exist, so 5000+ tracks scroll
  // smoothly), sortable (click = main key, shift-click adds keys), resizable and re-orderable
  // columns, multi-select with mouse and keyboard, context menu.
  // `rows` must already be filtered and sorted (TracksView does that).
  import { onMount, tick } from "svelte";
  import type { Track } from "$lib/api/types";
  import { mmss } from "$lib/format";
  import { labelEditor, labels } from "$lib/state/labels.svelte";
  import { playback } from "$lib/state/playback.svelte";
  import { session } from "$lib/state/session.svelte";
  import { armTrackDrag } from "$lib/state/trackDrag.svelte";
  import { holders } from "$lib/state/holders.svelte";
  import { suspectKinds } from "$lib/state/batchLogic";
  import { gridBatch } from "$lib/state/gridBatch.svelte";
  import { suspects } from "$lib/state/gridSuspects.svelte";
  import { jobs } from "$lib/state/jobs.svelte";
  import { tempo } from "$lib/state/tempo.svelte";
  import ContextMenu from "$lib/ui/ContextMenu.svelte";
  import Stars from "$lib/ui/Stars.svelte";
  import { gridTemplate, type ColDef, type ColKey } from "./columns";
  import ColourSwatch from "./ColourSwatch.svelte";
  import EnergyBar from "./EnergyBar.svelte";
  import MiniWave from "./MiniWave.svelte";
  import { addToCase } from "./addToCase.svelte";
  import { analysis } from "$lib/state/analysis.svelte";
  import { energy } from "$lib/state/energy.svelte";
  import { energySource } from "$lib/state/energySource.svelte";
  import { sections } from "$lib/state/sections.svelte";
  import EnergyCell from "../analysis/EnergyCell.svelte";
  import { addToMix, buildSet, editGrid, editMetadata, playTrack, rate, revealInExplorer } from "./trackActions";
  import { formatDate, formatSize, keyColour, parseKey } from "./trackLogic";
  import { caseMembers } from "$lib/state/caseMembers.svelte";
  import { caseLookup } from "./trackFilter";
  import type { TableState } from "./tableState.svelte";
  import { tracksState } from "./tracksState.svelte";

  let {
    rows,
    root,
    table = tracksState,
    label = "Tracks",
    dragFrom = "tracks",
    onopen,
    onadd,
    marker,
  }: {
    rows: Track[];
    root: string;
    /** This table's own state (sort, columns, filters, selection). The Tracks view's by default. */
    table?: TableState;
    label?: string;
    /** The drag scope name rows are dragged from ("tracks", or "mix:picker" in the Mix tab). */
    dragFrom?: string;
    /** Double-click / Enter on a row; without it the track is played. */
    onopen?: (t: Track) => void;
    /** "Add to mix" in the context menu; without it the context menu opens the Mix tab. */
    onadd?: (ids: number[]) => void;
    /** A short mark after the title (such as "x2" for a track already in the mix). */
    marker?: (id: number) => string;
  } = $props();

  const addToMixFn = (ids: number[]) => (onadd ? onadd(ids) : addToMix(ids));
  const caseNo = $derived(caseLookup(table.caseId, table.caseId === null ? null : caseMembers.tracksOf(table.caseId)));

  // the case columns and the case filter read the case lists, once per change of the case list
  $effect(() => {
    if (table.caseId !== null || table.prefs.visible.cases) void caseMembers.ensure();
  });

  const ROW = 28;
  const HEAD = 30;
  const OVERSCAN = 6;

  let scroller = $state<HTMLDivElement | null>(null);
  let scrollTop = $state(0);
  let viewH = $state(600);
  let anchorId: number | null = null;

  const cols = $derived(table.shownColumns);
  const widthOf = (c: ColDef) => table.prefs.widths[c.key];
  const totalW = $derived(cols.reduce((sum, c) => sum + widthOf(c), 0));
  let resizing = $state<ColKey | null>(null);
  const template = $derived(gridTemplate(cols, table.prefs.widths, table.prefs.fillWidth, resizing, table.prefs.pinned));
  const bodyH = $derived(Math.max(0, viewH - HEAD));
  const start = $derived(Math.max(0, Math.floor(scrollTop / ROW) - OVERSCAN));
  const end = $derived(Math.min(rows.length, Math.ceil((scrollTop + bodyH) / ROW) + OVERSCAN));
  const slice = $derived(rows.slice(start, end));
  const selSet = $derived(new Set(table.selectedIds));

  // ---- selection ------------------------------------------------------------------------

  const indexOfId = (id: number | null) => (id === null ? -1 : rows.findIndex((r) => r.track_id === id));

  function select(ids: number[], focus: number | null) {
    table.selectedIds = ids;
    table.focusId = focus;
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
      select(e.ctrlKey || e.metaKey ? [...new Set([...table.selectedIds, ...range])] : range, id);
    } else if (e.ctrlKey || e.metaKey) {
      anchorId = id;
      if (selSet.has(id)) {
        const rest = table.selectedIds.filter((x) => x !== id);
        select(rest, rest.at(-1) ?? null);
      } else {
        select([...table.selectedIds, id], id);
      }
    } else {
      anchorId = id;
      select([id], id);
    }
  }

  /** Select just this track and bring it into view (in the middle of the table when `center`);
   *  false when the filters hide it. */
  export function reveal(id: number, center = false): boolean {
    const i = indexOfId(id);
    if (i < 0) return false;
    anchorId = id;
    select([id], id);
    if (center && scroller) scroller.scrollTop = Math.max(0, i * ROW - (bodyH - ROW) / 2);
    else scrollToIndex(i);
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
      if (anchorId === null) anchorId = table.focusId ?? id;
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
    const cursor = indexOfId(table.focusId);
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
        select(rows.map((r) => r.track_id), table.focusId ?? rows[0]?.track_id ?? null);
        break;
      case "Escape": select([], null); break;
      case "Enter":
        if (table.focusId === null) return;
        if (onopen) {
          const t = rows[cursor];
          if (t) onopen(t);
        } else playTrack(table.focusId);
        break;
      case " ":
        if (table.focusId === null) return;
        if (playback.trackId === table.focusId) playback.toggle();
        else playTrack(table.focusId);
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
    const kept = table.selectedIds.filter((id) => visible.has(id));
    if (kept.length !== table.selectedIds.length) table.selectedIds = kept;
    if (table.focusId !== null && !visible.has(table.focusId)) table.focusId = kept.at(-1) ?? null;
  });

  onMount(async () => {
    await tick();
    const i = indexOfId(table.focusId);
    if (i >= 0) scrollToIndex(i);
  });

  // ---- sort and column order (header clicks, drag, keyboard) ----------------------------

  const sortKeys = $derived(table.prefs.sort);

  let headEl = $state<HTMLDivElement | null>(null);
  /** A header being dragged to a new place: the column, where it would land and the x of the marker. */
  let drag = $state<{ key: ColKey; before: ColKey | null; x: number } | null>(null);
  let suppressClick = false;

  function headerClick(e: MouseEvent, c: ColDef) {
    if (suppressClick || !c.sort) return;
    table.sortBy(c.sort, e.shiftKey);
  }

  /** Where a column dropped at `clientX` would go: before the first other column whose middle is
   *  right of the pointer (never before the heart), else at the end. */
  function dropTarget(clientX: number, key: ColKey): { before: ColKey | null; x: number } {
    const cells = [...(headEl?.querySelectorAll<HTMLElement>(".hcell") ?? [])].filter((el) => el.dataset.col !== key && el.dataset.col !== "fav");
    const left = headEl?.getBoundingClientRect().left ?? 0;
    for (const el of cells) {
      const r = el.getBoundingClientRect();
      if (clientX < r.left + r.width / 2) return { before: el.dataset.col as ColKey, x: r.left - left };
    }
    const last = cells.at(-1);
    return { before: null, x: last ? last.getBoundingClientRect().right - left : 0 };
  }

  function startHeaderDrag(e: PointerEvent, c: ColDef) {
    if (e.button !== 0 || c.key === "fav" || (e.target instanceof Element && e.target.closest(".grip"))) return;
    const el = e.currentTarget as HTMLElement;
    const x0 = e.clientX;
    let active = false;
    // captured from the start so a fast flick across other headers still reaches this handler
    try {
      el.setPointerCapture(e.pointerId);
    } catch {
      // the pointer is already gone; a plain click still sorts
    }
    const move = (m: PointerEvent) => {
      if (!active && Math.abs(m.clientX - x0) < 6) return; // a click, not a drag
      active = true;
      drag = { key: c.key, ...dropTarget(m.clientX, c.key) };
    };
    const finish = (commit: boolean) => {
      el.removeEventListener("pointermove", move);
      el.removeEventListener("pointerup", up);
      el.removeEventListener("pointercancel", cancel);
      const d = drag;
      drag = null;
      if (!active) return;
      suppressClick = true; // the click that follows the drop must not sort
      setTimeout(() => (suppressClick = false), 0);
      if (commit && d) table.moveColumn(d.key, d.before);
    };
    const up = () => finish(true);
    const cancel = () => finish(false);
    el.addEventListener("pointermove", move);
    el.addEventListener("pointerup", up);
    el.addEventListener("pointercancel", cancel);
  }

  async function headerKey(e: KeyboardEvent, c: ColDef) {
    if (e.altKey && (e.key === "ArrowLeft" || e.key === "ArrowRight")) {
      e.preventDefault();
      e.stopPropagation();
      if (table.stepColumn(c.key, e.key === "ArrowLeft" ? -1 : 1)) {
        await tick(); // moving the element in the page drops its focus
        headEl?.querySelector<HTMLElement>(`[data-col="${c.key}"]`)?.focus();
      }
      return;
    }
    if (e.key !== "Enter" && e.key !== " ") return;
    e.stopPropagation();
    if (!c.sort) return;
    e.preventDefault();
    table.sortBy(c.sort, e.shiftKey);
  }

  // ---- column resize --------------------------------------------------------------------

  function startResize(e: PointerEvent, c: ColDef) {
    e.preventDefault();
    e.stopPropagation();
    const handle = e.currentTarget as HTMLElement;
    handle.setPointerCapture(e.pointerId);
    const x0 = e.clientX;
    // a Title/Artist column that is filling a wide window is drawn wider than its saved width:
    // the drag starts from what is on screen. Nothing is saved until the pointer moves; on
    // release the column stays at the width it ended on (pinned), the other one takes the spare room.
    const drawn = (handle.closest('[role="columnheader"]') as HTMLElement | null)?.getBoundingClientRect().width;
    const w0 = drawn && drawn > 0 ? Math.round(drawn) : widthOf(c);
    let moved = false;
    const move = (m: PointerEvent) => {
      if (!moved && m.clientX === x0) return;
      // fix the column to px in the same render that gives it its dragged width, so pressing the grip never jumps it
      moved = true;
      resizing = c.key;
      table.setWidth(c.key, w0 + m.clientX - x0);
    };
    const stop = () => {
      if (moved) table.pinColumn(c.key);
      resizing = null;
      handle.removeEventListener("pointermove", move);
      handle.removeEventListener("pointerup", stop);
      handle.removeEventListener("pointercancel", stop);
      if (moved) table.save();
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
    menu = { x: Math.max(0, Math.min(e.clientX, innerWidth - 230)), y: Math.max(0, Math.min(e.clientY, innerHeight - 550)), ids: [...table.selectedIds] };
  }

  const menuItems = $derived.by(() => {
    if (!menu) return [];
    const ids = menu.ids;
    const one = ids.length === 1;
    const n = ids.length;
    const allFav = ids.every((id) => table.favourites.has(id));
    const again = ids.every((id) => energy.of(id) !== null);
    const sectionsAgain = ids.every((id) => sections.of(id) !== null);
    return [
      { label: "Play", onclick: () => playTrack(ids[0]) },
      { label: one ? "Edit metadata…" : `Edit metadata of ${n} tracks…`, onclick: () => editMetadata(ids, rows.map((r) => r.track_id)) },
      { label: one ? "Tags, colour, energy…" : `Tags, colour, energy of ${n} tracks…`, onclick: () => labelEditor.edit(ids) },
      { label: one ? "Add to mix" : `Add ${n} tracks to mix`, onclick: () => addToMixFn(ids) },
      { label: one ? "Add to case…" : `Add ${n} tracks to case…`, onclick: () => addToCase.ask(ids) },
      { label: "Which cases hold this track…", onclick: () => holders.show(ids[0]), disabled: !one },
      { label: one ? "Build a set…" : `Build a set from ${n} tracks…`, onclick: () => buildSet(ids), disabled: one },
      {
        label: allFav ? (one ? "Remove from favourites" : `Remove ${n} from favourites`) : one ? "Favourite" : `Favourite ${n} tracks`,
        onclick: () => tracksState.setFavourite(ids, !allFav),
      },
      // key and tempo tools: each opens its own dialog or confirmation naming every track it touches
      { label: one ? "Key: detect…" : `Key: detect for ${n} tracks…`, onclick: () => analysis.askDetectKeys(ids) },
      { label: one ? "Key: set…" : `Key: set for ${n} tracks…`, onclick: () => analysis.askSetKey(ids) },
      { label: "BPM: set…", onclick: () => tempo.askSet(ids[0]), disabled: !one },
      { label: one ? "BPM: halve…" : `BPM: halve ${n} tracks…`, onclick: () => tempo.askHalve(ids) },
      { label: one ? "BPM: double…" : `BPM: double ${n} tracks…`, onclick: () => tempo.askDouble(ids) },
      { label: one ? "Re-analyse beat grid…" : `Re-analyse ${n} tracks…`, onclick: () => jobs.askReanalyse(ids) },
      { label: one ? "Beat grid tools (preview)…" : `Beat grid tools for ${n} tracks (preview)…`, onclick: () => gridBatch.open(ids) },
      // app-side analyses (nothing is written to the device); tracks that all have a result can only mean "measure again"
      {
        label: `Energy (auto): analyse${again ? " again" : ""}${one ? "" : ` ${n} tracks`}…`,
        onclick: () => energy.askAnalyse(ids, again),
        disabled: !jobs.canWrite,
      },
      {
        label: `Sections: detect${sectionsAgain ? " again" : ""}${one ? "" : ` for ${n} tracks`}…`,
        onclick: () => sections.askAnalyse(ids, sectionsAgain),
        disabled: !jobs.canWrite,
      },
      { label: "Edit grid", onclick: () => editGrid(ids[0]), disabled: !one },
      { label: "Show in Explorer", onclick: () => revealInExplorer(ids[0]), disabled: !one },
    ];
  });

  const stop = (e: Event) => e.stopPropagation();

  // Dragging rows carries the selected tracks (or just this one) so a case can take them: the
  // drop strip in the sidebar (cases area) lists the cases while a drag is running. Read when the
  // drag starts (pointer events, see state/trackDrag.svelte.ts).
  function dragRows(t: Track) {
    if (!selSet.has(t.track_id)) {
      anchorId = t.track_id;
      select([t.track_id], t.track_id);
      return { ids: [t.track_id] };
    }
    return { ids: [...table.selectedIds] };
  }
</script>

<!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
<div
  class="scroller"
  role="grid"
  aria-label={label}
  aria-multiselectable="true"
  aria-rowcount={rows.length}
  tabindex="0"
  bind:this={scroller}
  bind:clientHeight={viewH}
  onscroll={(e) => (scrollTop = e.currentTarget.scrollTop)}
  onkeydown={onKey}
>
  <div class="head" role="row" bind:this={headEl} style:grid-template-columns={template} style:min-width="{totalW}px" style:height="{HEAD}px">
    {#each cols as c (c.key)}
      {@const rank = c.sort ? sortKeys.findIndex((k) => k.field === c.sort) : -1}
      <div
        class="hcell"
        class:num={c.num}
        class:sortable={!!c.sort}
        class:dragging={drag?.key === c.key}
        data-col={c.key}
        role="columnheader"
        tabindex={c.sort ? 0 : -1}
        aria-sort={rank === 0 ? (sortKeys[0].asc ? "ascending" : "descending") : "none"}
        title={c.sort ? "Click to sort, shift-click to add a second sort key, drag to move, Alt+Left / Right to move with the keyboard" : "Drag to move"}
        onpointerdown={(e) => startHeaderDrag(e, c)}
        onclick={(e) => headerClick(e, c)}
        onkeydown={(e) => headerKey(e, c)}
      >
        {#if c.key === "fav"}<span class="heart-h" title="Favourites">♥</span>{:else}<span class="lbl">{c.label}</span>{/if}
        {#if rank >= 0}
          <span class="arrow">{sortKeys[rank].asc ? "▲" : "▼"}</span>
          {#if sortKeys.length > 1}<span class="ord" title="Sort key {rank + 1} of {sortKeys.length}">{rank + 1}</span>{/if}
        {/if}
        <!-- svelte-ignore a11y_click_events_have_key_events -->
        <span class="grip" role="presentation" onpointerdown={(e) => startResize(e, c)} onclick={stop} title="Drag to resize"></span>
      </div>
    {/each}
    {#if drag}<span class="drop" style:left="{drag.x}px" aria-hidden="true"></span>{/if}
  </div>

  <div class="body" style:height="{rows.length * ROW}px" style:min-width="{totalW}px">
    {#each slice as t, k (t.track_id)}
      {@const i = start + k}
      {@const need = session.needs.has(t.track_id)}
      {@const fav = table.favourites.has(t.track_id)}
      {@const lab = labels.get(t.track_id)}
      {@const selected = selSet.has(t.track_id)}
      <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
      <div
        class="row"
        class:alt={i % 2 === 1}
        class:sel={selected}
        class:dim={need}
        class:focus={t.track_id === table.focusId}
        role="row"
        tabindex="-1"
        aria-selected={selected}
        aria-rowindex={i + 1}
        style:top="{i * ROW}px"
        style:height="{ROW}px"
        style:grid-template-columns={template}
        onclick={(e) => clickRow(e, t, i)}
        ondblclick={() => (onopen ? onopen(t) : playTrack(t.track_id))}
        oncontextmenu={(e) => openMenu(e, t)}
        onpointerdown={(e) => armTrackDrag(e, dragFrom, () => dragRows(t))}
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
            {:else if c.key === "colour"}
              <ColourSwatch colour={lab?.colour ?? null} size={14} />
            {:else if c.key === "energy"}
              <EnergyBar energy={lab?.energy ?? null} dim={lab?.energy != null && energySource.used(t.track_id) !== "mine"} />
            {:else if c.key === "tags"}
              {#if lab?.tags.length}
                <span class="tags" title={lab.tags.join(", ")}>{#each lab.tags as tag (tag)}<span class="tag">{tag}</span>{/each}</span>
              {/if}
            {:else if c.key === "title"}
              {#if playback.trackId === t.track_id}<span class="now" title="Loaded in the player">▶</span>{/if}
              <span class="text">{t.title}</span>
              {#if marker?.(t.track_id)}<em class="used" title="Already in this mix">{marker(t.track_id)}</em>{/if}
              {#if need}<span class="badge" title={session.needs.get(t.track_id)}>needs analysis</span>{/if}
            {:else if c.key === "artist"}
              <span class="text">{t.artist}</span>
            {:else if c.key === "album"}
              <span class="text">{t.album}</span>
            {:else if c.key === "genre"}
              <span class="text">{t.genre}</span>
            {:else if c.key === "year"}
              {t.year > 0 ? t.year : ""}
            {:else if c.key === "label"}
              <span class="text">{t.label}</span>
            {:else if c.key === "remixer"}
              <span class="text">{t.remixer}</span>
            {:else if c.key === "composer"}
              <span class="text">{t.composer}</span>
            {:else if c.key === "comments"}
              <span class="text" title={t.comments}>{t.comments}</span>
            {:else if c.key === "added"}
              {formatDate(t.date_added)}
            {:else if c.key === "lastplayed"}
              {formatDate(t.last_played)}
            {:else if c.key === "format"}
              {t.format}
            {:else if c.key === "bitrate"}
              {t.bit_rate > 0 ? `${t.bit_rate} kbps` : ""}
            {:else if c.key === "size"}
              {formatSize(t.file_size)}
            {:else if c.key === "wave"}
              {#if t.location}<MiniWave {root} location={t.location} trackId={t.track_id} />{/if}
            {:else if c.key === "bpm"}
              {@const sus = suspects.byId.get(t.track_id)}
              {#if sus && !need}<span class="suspect" title={`Suspect grid: ${suspectKinds(sus)}\n${sus.reasons.join("\n")}`} aria-label="Suspect grid">⚑</span>{/if}
              {t.bpm && !need ? t.bpm.toFixed(2) : "–"}
            {:else if c.key === "key"}
              {@const k = parseKey(t.key)}
              {#if k}
                <span class="key" style:color={keyColour(k)} style:background="color-mix(in srgb, {keyColour(k)} 14%, transparent)" title={t.key}>{k.code}</span>
              {/if}
            {:else if c.key === "autoenergy"}
              <EnergyCell entry={energy.entry(t.track_id)} dim={energy.entry(t.track_id) !== null && energySource.used(t.track_id) !== "auto"} />
            {:else if c.key === "rating"}
              <span class="rate" role="presentation" onclick={stop} ondblclick={stop}>
                <Stars rating={t.rating} onchange={(n) => rate(t, n)} />
              </span>
            {:else if c.key === "plays"}
              {t.times_played || ""}
            {:else if c.key === "time"}
              {mmss(t.play_time_secs)}
            {:else if c.key === "cases"}
              {@const names = caseMembers.namesOf(t.track_id)}
              {#if names.length}<span class="text" title={names.join(", ")}>{names.join(", ")}</span>{/if}
            {:else if c.key === "caseno"}
              {@const pos = caseNo.position(t.track_id)}
              {pos === null ? "" : pos + 1}
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
  /* a narrow column gives up the end of its name, never the sort marks */
  .lbl {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .arrow {
    flex: none;
    font-size: 9px;
    color: var(--accent);
  }
  /* the position of this column in a multi-key sort */
  .ord {
    flex: none;
    min-width: 13px;
    padding: 0 3px;
    border-radius: 7px;
    background: var(--accent-soft);
    color: var(--accent);
    font-size: 10px;
    line-height: 13px;
    text-align: center;
  }
  .hcell.dragging {
    opacity: 0.45;
  }
  /* where the dragged column would land */
  .drop {
    position: absolute;
    top: 3px;
    bottom: 3px;
    width: 2px;
    margin-left: -1px;
    background: var(--accent);
    pointer-events: none;
    z-index: 3;
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
    /* a double-click plays the track and shift-click selects a range: neither should highlight text */
    user-select: none;
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
  .used {
    flex: none;
    font-style: normal;
    font-size: 11px;
    color: var(--green);
  }
  .badge {
    flex: none;
    padding: 0 6px;
    border-radius: 8px;
    font-size: 11px;
    background: var(--accent-soft);
    color: var(--accent);
  }
  .suspect {
    margin-right: 4px;
    color: var(--accent);
    cursor: help;
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
  .tags {
    display: flex;
    gap: 4px;
    min-width: 0;
    overflow: hidden;
  }
  .tag {
    flex: none;
    padding: 0 6px;
    border-radius: 8px;
    font-size: 11px;
    background: var(--hover);
    color: var(--text);
  }
</style>
