<script lang="ts">
  import { untrack } from "svelte";
  import { chooseLibraryFolder } from "$lib/api/library";
  import { libraryKind, libraryLabel, sameRoot } from "$lib/state/place";
  import { session } from "$lib/state/session.svelte";
  import { armTrackDrag, trackDrag, trackDropTarget, type DropOptions } from "$lib/state/trackDrag.svelte";
  import Button from "$lib/ui/Button.svelte";
  import ContextMenu from "$lib/ui/ContextMenu.svelte";
  import { libraryOptions } from "../sync/syncLogic";
  import { libs } from "./libraries.svelte";
  import {
    clickSelect, clock, copyLabel, defaultRight, dragFrom, dragIds, dragLabel, intoCaseLabel, num, onBoth, otherSide, parseDragFrom, presenceOf, pruneTo, readButtonText,
    resolveDrop, rightOptions, rowId, rowsOf, statusText, TABS, type Row, type Side, type Tab,
  } from "./librariesLogic";
  import type { Collection, Track } from "$lib/api/types";

  // One side of the two-pane view: which library, its Tracks / Mixes / Cases, a search, and a
  // windowed list (only the rows in view are drawn, so 30 000 tracks stay quick). Rows are picked
  // with click, Shift-click and Ctrl-click and dragged to the other pane with the pointer (see
  // trackDrag.svelte.ts); "Copy to ..." and the row menu do the same by click.
  let { side }: { side: Side } = $props();

  const ROW = 26;
  const OVERSCAN = 8;

  const data = $derived(libs.data(side));
  const otherData = $derived(libs.data(otherSide(side)));
  const there = $derived(otherData ? presenceOf(otherData) : null);
  const tab = $derived<Tab>(libs.tab[side]);
  const query = $derived(libs.search[side]);
  const rows = $derived<readonly Row[]>(data ? rowsOf(data, tab, query) : []);
  const visibleIds = $derived(rows.map((r) => rowId(tab, r)));
  const sel = $derived(libs.selected[side][tab]);
  const total = $derived(data ? data[tab].length : 0);

  // ---- names --------------------------------------------------------------------------------

  const options = $derived(libraryOptions(session.computer, session.volumes, session.library ? { root: session.library.root, kind: session.library.kind } : null));
  const open = $derived(session.library ? { root: session.library.root, kind: session.library.kind } : null);
  const choices = $derived.by(() => {
    const all = [...options];
    for (const f of libs.folders) if (!all.some((o) => sameRoot(o.value, f.value))) all.push(f);
    return all;
  });
  const picker = $derived(rightOptions(choices, open));
  const chosen = $derived(choices.find((o) => sameRoot(o.value, libs.rightRoot)));
  const nameOf = (d: { root: string; kind: "pacemaker" | "computer" } | null) => (d ? libraryLabel(d.kind, d.root, session.computer?.path ?? null) : "the other library");
  const otherName = $derived(nameOf(otherData ?? (side === "left" ? (chosen ? { root: chosen.value, kind: chosen.kind } : null) : open)));

  // the right pane follows the picker: keep a choice that is still on offer, else the default
  $effect(() => {
    if (side !== "right") return;
    const want = defaultRight(choices, open, libs.rightRoot);
    if (want !== libs.rightRoot) libs.chooseRight(want);
  });

  async function pickFolder() {
    const dir = await chooseLibraryFolder();
    if (!dir) return;
    const kind = libraryKind(dir);
    libs.addFolder(dir, libraryLabel(kind, dir, session.computer?.path ?? null), kind, dir);
  }

  async function onPick(el: HTMLSelectElement) {
    if (el.value !== "__folder__") return libs.chooseRight(el.value);
    await pickFolder();
    el.value = libs.rightRoot; // a cancelled dialog leaves the picker where it was
  }

  // ---- the window of rows ---------------------------------------------------------------------

  let scroller = $state<HTMLDivElement | null>(null);
  let scrollTop = $state(0);
  let viewH = $state(300);
  const start = $derived(Math.max(0, Math.floor(scrollTop / ROW) - OVERSCAN));
  const end = $derived(Math.min(rows.length, Math.ceil((scrollTop + viewH) / ROW) + OVERSCAN));
  const slice = $derived(rows.slice(start, end));

  // a new tab, search or library starts at the top
  $effect(() => {
    void tab;
    void query;
    void data?.root;
    if (scroller) scroller.scrollTop = 0;
  });

  // a search takes the rows it hides out of the selection
  $effect(() => {
    const ids = visibleIds;
    const cur = untrack(() => libs.selected[side][tab]);
    if (cur.size === 0) return;
    const kept = pruneTo(cur, ids);
    if (kept.size !== cur.size) libs.setSelection(side, tab, kept, null);
  });

  // ---- selection ------------------------------------------------------------------------------

  let cursor = $state(-1);

  function pick(e: MouseEvent, i: number) {
    const r = clickSelect(sel, libs.anchor[side][tab], visibleIds, i, { shift: e.shiftKey, ctrl: e.ctrlKey || e.metaKey });
    libs.setSelection(side, tab, r.selected, r.anchor);
    cursor = i;
  }

  function reveal(i: number) {
    if (!scroller) return;
    const top = i * ROW;
    if (top < scroller.scrollTop) scroller.scrollTop = top;
    else if (top + ROW > scroller.scrollTop + viewH - ROW) scroller.scrollTop = top + ROW - viewH + ROW;
  }

  function onKey(e: KeyboardEvent) {
    const n = rows.length;
    if (n === 0 || e.altKey) return;
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "a") {
      e.preventDefault();
      libs.setSelection(side, tab, new Set(visibleIds), visibleIds[0] ?? null);
      return;
    }
    if (e.key === "Escape" && sel.size > 0) {
      libs.setSelection(side, tab, new Set(), null);
      return;
    }
    const page = Math.max(1, Math.floor(viewH / ROW) - 1);
    const at = cursor < 0 ? 0 : cursor;
    const to =
      e.key === "ArrowDown" ? (cursor < 0 ? 0 : at + 1) :
      e.key === "ArrowUp" ? at - 1 :
      e.key === "PageDown" ? at + page :
      e.key === "PageUp" ? at - page :
      e.key === "Home" ? 0 :
      e.key === "End" ? n - 1 : null;
    if (to === null) return;
    e.preventDefault();
    const i = Math.max(0, Math.min(n - 1, to));
    const r = clickSelect(sel, libs.anchor[side][tab], visibleIds, i, { shift: e.shiftKey, ctrl: false });
    libs.setSelection(side, tab, r.selected, r.anchor);
    cursor = i;
    reveal(i);
  }

  // ---- drag out -------------------------------------------------------------------------------

  /** Read when a drag from `id`'s row actually starts: the selection if the row is in it, else the
   *  row alone (which the press selects). */
  function dragSource(id: number) {
    const ids = dragIds(sel, id, visibleIds);
    if (!sel.has(id)) libs.setSelection(side, tab, new Set([id]), id);
    return { ids, label: dragLabel(tab, ids.length) };
  }

  // ---- drop in --------------------------------------------------------------------------------

  const caseNameAt = (hit: Element) => hit.closest<HTMLElement>("[data-case-name]")?.dataset.caseName ?? null;

  const dropOptions: DropOptions = {
    scope: "lib",
    // a pane takes drags from the other pane only, and only once its library is showing
    accept: () => {
      const f = parseDragFrom(trackDrag.from);
      return !!f && f.side !== side && !!data && !libs.busy;
    },
    onhover: (hit) => {
      if (!hit) {
        if (libs.over?.side === side) libs.over = null;
        return;
      }
      const f = parseDragFrom(trackDrag.from);
      const caseName = tab === "cases" && f?.tab === "tracks" ? caseNameAt(hit) : null;
      if (libs.over?.side !== side || libs.over.caseName !== caseName) libs.over = { side, caseName };
    },
    ondrop: (ids, hit, _at, from) => {
      libs.over = null;
      const f = parseDragFrom(from);
      if (!f) return;
      const r = resolveDrop({ fromSide: f.side, fromTab: f.tab, overSide: side, overTab: tab, overCase: caseNameAt(hit) });
      if (r) void libs.copy(f.side, f.tab, ids, r.into);
    },
  };

  // ---- copy by click ----------------------------------------------------------------------------

  const canCopyNow = $derived(sel.size > 0 && !!otherData && !libs.busy);
  const copyWhy = $derived(sel.size === 0 ? `Select ${tab} here first, then copy them to ${otherName}` : !otherData ? `Read ${otherName} first` : "");
  const copyText = $derived(copyLabel(side, otherName));
  const copySelected = () => void libs.copy(side, tab, sel);
  // tracks can also go into a case of the other library by click (a drop on a case row does the same)
  const intoText = $derived(intoCaseLabel(otherName));
  const intoSelected = () => libs.openChooser(side, sel);

  let menu = $state<{ x: number; y: number } | null>(null);
  function rowMenu(e: MouseEvent, id: number, i: number) {
    e.preventDefault();
    if (!sel.has(id)) {
      libs.setSelection(side, tab, new Set([id]), id);
      cursor = i;
    }
    menu = { x: e.clientX, y: e.clientY };
  }

  // the Time column only pays for h:mm:ss when this list has a track or mix of an hour or more
  const longTimes = $derived(tab !== "cases" && rows.some((r) => (r as Track | Collection).play_time_secs >= 3600));
  const clockOf = (r: Row) => clock(tab === "tracks" ? (r as Track).play_time_secs : (r as Collection).play_time_secs);
</script>

<section
  class="pane"
  class:over={libs.over?.side === side}
  aria-label={side === "left" ? "Open library" : "Other library"}
  use:trackDropTarget={dropOptions}
>
  <div class="phead">
    {#if side === "left"}
      {#if data}
        <div class="who"><b title={data.root}>{nameOf(data)}</b><span class="muted">open</span></div>
      {:else}
        <div class="who"><b>No library open</b></div>
      {/if}
    {:else}
      <select
        class="pick"
        aria-label="Library on the right"
        value={libs.rightRoot}
        disabled={libs.busy}
        onchange={(e) => onPick(e.currentTarget)}
      >
        {#if !libs.rightRoot}<option value="" disabled>Choose a library…</option>{/if}
        {#each picker as o (o.value)}<option value={o.value} title={o.title}>{o.label}</option>{/each}
        <option value="__folder__">Open folder…</option>
      </select>
      <Button small variant="ghost" title="Look for a connected Pacemaker. What was read of the other library is forgotten" disabled={libs.busy} onclick={() => libs.rescan()}>Rescan</Button>
      {#if data}
        <Button small variant="ghost" title={`Read ${chosen?.label ?? "the library"} again`} disabled={libs.busy || !!libs.reading} onclick={() => libs.read(libs.rightRoot)}>Read again</Button>
      {/if}
    {/if}
  </div>

  {#if !data}
    <div class="empty">
      {#if side === "left"}
        <p>Open a library to see it here.</p>
        {#if session.computer?.exists}
          <Button variant="primary" onclick={() => session.computer && session.load(session.computer.path)}>Open the library on this computer</Button>
        {/if}
      {:else if !libs.rightRoot && picker.length > 0}
        <p>Choose a library above to see it here.</p>
      {:else if !libs.rightRoot}
        <p>There is no other library to show. Plug the Pacemaker into this computer (it appears as a drive) and press Rescan, or open a folder that holds a library.</p>
        <div class="row-actions">
          <Button variant="primary" onclick={() => libs.rescan()}>Rescan</Button>
          <Button onclick={pickFolder}>Open folder…</Button>
        </div>
      {:else if libs.reading}
        <p class="muted" role="status">Reading {chosen?.label ?? "the library"}…</p>
      {:else if libs.readError}
        <p class="warn" role="alert">{libs.readError}</p>
        <Button onclick={() => libs.read(libs.rightRoot)}>Try again</Button>
      {:else}
        <p>
          {chosen?.label ?? "This library"} is read only when you ask. Reading lists its tracks, mixes and cases from its database; no audio
          is touched. Drops on this side wait until it is read.
        </p>
        <Button variant="primary" onclick={() => libs.read(libs.rightRoot)}>{readButtonText(libs.rightRoot)}</Button>
      {/if}
    </div>
  {:else}
    <div class="tabs" role="tablist" aria-label="What to list">
      {#each TABS as t (t.id)}
        <button
          role="tab"
          class:active={tab === t.id}
          aria-selected={tab === t.id}
          onclick={() => (libs.tab = { ...libs.tab, [side]: t.id })}
        >{t.label} <span class="muted">{num(data[t.id].length)}</span></button>
      {/each}
    </div>
    <input
      class="search"
      type="search"
      placeholder={`Search ${tab}…`}
      aria-label={`Search ${tab}`}
      value={query}
      oninput={(e) => (libs.search = { ...libs.search, [side]: e.currentTarget.value })}
    />

    <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
    <div
      class="scroller"
      class:tracks={tab === "tracks"}
      class:cols-mix={tab === "mixes"}
      class:cols-case={tab === "cases"}
      class:long={longTimes}
      role="listbox"
      aria-multiselectable="true"
      aria-label={`${tab} of ${nameOf(data)}`}
      tabindex="0"
      bind:this={scroller}
      bind:clientHeight={viewH}
      style:--both={there ? "34px" : "0px"}
      onscroll={(e) => (scrollTop = e.currentTarget.scrollTop)}
      onkeydown={onKey}
    >
      <div class="head" role="presentation">
        <span></span>
        {#if tab === "tracks"}
          <span>Title</span><span class="artist">Artist</span><span class="r">BPM</span><span>Key</span><span class="r">Time</span>
        {:else if tab === "mixes"}
          <span>Name</span><span class="r">Time</span><span class="r">Tracks</span>
        {:else}
          <span>Name</span><span class="r">Tracks</span>
        {/if}
      </div>
      <div class="body" style:height="{rows.length * ROW}px">
        {#each slice as r, k (rowId(tab, r))}
          {@const i = start + k}
          {@const id = rowId(tab, r)}
          {@const both = onBoth(tab, r, there)}
          <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
          <div
            class="row"
            class:alt={i % 2 === 1}
            class:sel={sel.has(id)}
            class:cur={i === cursor}
            class:drop={tab === "cases" && libs.over?.side === side && libs.over.caseName === (r as Collection).name}
            role="option"
            tabindex="-1"
            aria-selected={sel.has(id)}
            style:top="{i * ROW}px"
            style:height="{ROW}px"
            data-case-name={tab === "cases" ? (r as Collection).name : undefined}
            onclick={(e) => pick(e, i)}
            oncontextmenu={(e) => rowMenu(e, id, i)}
            onpointerdown={(e) => armTrackDrag(e, dragFrom(side, tab), () => dragSource(id))}
          >
            <span class="c both-c">{#if both}<span class="both" title={`Also on ${otherName}`}>both</span>{/if}</span>
            {#if tab === "tracks"}
              {@const t = r as Track}
              <span class="c" title={t.title}>{t.title}</span>
              <span class="c artist" title={t.artist}>{t.artist}</span>
              <span class="c r num">{t.bpm > 0 ? Math.round(t.bpm) : ""}</span>
              <span class="c">{t.key}</span>
              <span class="c r num">{clockOf(r)}</span>
            {:else if tab === "mixes"}
              {@const m = r as Collection}
              <span class="c" title={m.name}>{m.name}</span>
              <span class="c r num">{clockOf(r)}</span>
              <span class="c r num">{m.track_count}</span>
            {:else}
              {@const c = r as Collection}
              <span class="c" title={c.name}>{c.name}</span>
              <span class="c r num">{c.track_count}</span>
            {/if}
          </div>
        {/each}
        {#if rows.length === 0}
          <p class="none muted">{query.trim() ? `No ${tab} match “${query.trim()}”.` : `No ${tab} in this library.`}</p>
        {/if}
      </div>
    </div>

    <div class="foot">
      <div class="status muted" role="status">{statusText(tab, sel.size, rows.length, total)}</div>
      <div class="acts">
        {#if tab === "tracks"}<Button small disabled={!canCopyNow} title={copyWhy || intoText} onclick={intoSelected}>Into a case…</Button>{/if}
        <Button small disabled={!canCopyNow} title={copyWhy} onclick={copySelected}>{copyText}</Button>
      </div>
    </div>
  {/if}
</section>

{#if menu}
  <ContextMenu
    x={menu.x}
    y={menu.y}
    items={[
      { label: copyText, onclick: copySelected, disabled: !canCopyNow },
      ...(tab === "tracks" ? [{ label: intoText, onclick: intoSelected, disabled: !canCopyNow }] : []),
    ]}
    onclose={() => (menu = null)}
  />
{/if}

<style>
  .pane {
    container-type: inline-size;
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
    border: 1px solid var(--line);
    border-radius: 6px;
    background: var(--panel);
    overflow: hidden;
  }
  .pane.over {
    border-color: var(--accent);
    box-shadow: 0 0 0 1px var(--accent) inset;
  }
  .phead {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 8px 10px;
    border-bottom: 1px solid var(--line);
    /* the same height whether it holds a name or the picker, so the two panes line up */
    box-sizing: border-box;
    height: 46px;
    flex: none;
  }
  .who {
    display: flex;
    align-items: baseline;
    gap: 8px;
    min-width: 0;
  }
  .who b {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .pick {
    flex: 1;
    min-width: 0;
    background: var(--bg);
    color: var(--text);
    border: 1px solid var(--line-hi);
    border-radius: 4px;
    padding: 4px 6px;
    font: inherit;
  }
  .empty {
    flex: 1;
    padding: 16px 14px;
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 10px;
  }
  .empty p {
    margin: 0;
  }
  .row-actions {
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
  }
  .warn {
    color: var(--bad);
  }
  .tabs {
    display: flex;
    gap: 2px;
    padding: 6px 8px 0;
  }
  .tabs button {
    background: none;
    border: 1px solid transparent;
    border-bottom: none;
    border-radius: 4px 4px 0 0;
    color: var(--text);
    padding: 4px 9px;
    font: inherit;
    cursor: pointer;
  }
  .tabs button:hover {
    background: var(--hover);
  }
  .tabs button.active {
    background: var(--accent-soft);
    color: var(--accent);
    border-color: var(--line-hi);
  }
  .search {
    margin: 0 8px 6px;
    background: var(--bg);
    color: var(--text);
    border: 1px solid var(--line-hi);
    border-radius: 4px;
    padding: 4px 8px;
    font: inherit;
    min-width: 0;
  }
  .scroller {
    position: relative;
    flex: 1;
    min-height: 150px;
    overflow: auto;
    border-top: 1px solid var(--line);
    outline: none;
    scrollbar-gutter: stable;
    --cols: var(--both) minmax(60px, 1fr) 44px 38px;
    --both: 34px;
    --time: 44px;
  }
  .scroller.long {
    --time: 62px;
  }
  .scroller.tracks {
    --cols: var(--both) minmax(70px, 1.5fr) minmax(48px, 1fr) 36px 46px var(--time);
  }
  .scroller.cols-mix {
    --cols: var(--both) minmax(70px, 1fr) var(--time) 48px;
  }
  .scroller.cols-case {
    --cols: var(--both) minmax(70px, 1fr) 54px;
  }
  /* a narrow pane (the window at its smallest) drops the artist column so titles stay readable */
  @container (max-width: 400px) {
    .scroller.tracks {
      --cols: var(--both) minmax(70px, 1fr) 36px 46px var(--time);
    }
    .artist {
      display: none;
    }
  }
  .scroller:focus-visible {
    box-shadow: 0 0 0 1px var(--accent) inset;
  }
  .head {
    position: sticky;
    top: 0;
    z-index: 2;
    display: grid;
    grid-template-columns: var(--cols);
    height: 24px;
    align-items: center;
    background: var(--deep);
    border-bottom: 1px solid var(--line);
    color: var(--muted);
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }
  .head span {
    padding: 0 6px;
    white-space: nowrap;
    overflow: hidden;
  }
  .body {
    position: relative;
  }
  .row {
    position: absolute;
    left: 0;
    right: 0;
    display: grid;
    grid-template-columns: var(--cols);
    align-items: center;
    background: var(--row);
    cursor: default;
    user-select: none;
  }
  .row.alt {
    background: var(--row-alt);
  }
  .row:hover {
    background: var(--hover);
  }
  .row.sel {
    background: var(--accent-soft);
  }
  .row.cur {
    outline: 1px solid var(--line-hi);
    outline-offset: -1px;
  }
  .row.drop {
    outline: 2px solid var(--accent);
    outline-offset: -2px;
    background: var(--accent-soft);
  }
  .c {
    padding: 0 6px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .r {
    text-align: right;
  }
  .num {
    font-variant-numeric: tabular-nums;
  }
  .both-c {
    padding: 0 0 0 6px;
  }
  .both {
    display: inline-block;
    padding: 0 5px;
    font-size: 10px;
    line-height: 15px;
    color: var(--muted);
    border: 1px solid var(--line-hi);
    border-radius: 8px;
  }
  .none {
    margin: 0;
    padding: 14px 10px;
  }
  .foot {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: 6px 10px;
    padding: 7px 10px;
    border-top: 1px solid var(--line);
  }
  .status {
    font-size: 12px;
    min-width: 0;
  }
  .acts {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
</style>
