<script lang="ts">
  // The mixes on the device: search, sort, pick one or several, then open, rename, duplicate,
  // delete, import and export .pmk. Bulk actions (delete, export) act on exactly the rows ticked
  // and shown, and their confirmations list every one.
  import type { MixSummary } from "$lib/api/mixes";
  import { mixes } from "$lib/state/mixes.svelte";
  import { session } from "$lib/state/session.svelte";
  import { toast } from "$lib/state/toast.svelte";
  import Button from "$lib/ui/Button.svelte";
  import Stars from "$lib/ui/Stars.svelte";
  import Table from "$lib/ui/Table.svelte";
  import { errText } from "./actions";
  import { deleteMixes, duplicateMix, exportMixes, exportPmk, importPmk, newMix, openMix, reloadList, renameMix } from "./flow";
  import { mmss } from "./layout";
  import { listMeta } from "./listmeta.svelte";
  import { listPrefs } from "./listPrefs.svelte";
  import { SORT_LABELS, clickRow, defaultAscending, filterMixes, prune, sortMixes, toggleRow, type SortKey } from "./mixlist";
  import { mv } from "./view.svelte";

  let selected = $state.raw<ReadonlySet<number>>(new Set());
  let anchor = $state<number | null>(null);
  let query = $state("");
  let loading = $state(true);

  // refresh when the library changes (an in-place reload keeps the view)
  $effect(() => {
    void session.library;
    loading = true;
    reloadList()
      .catch((e) => toast.show(`Could not list the mixes: ${errText(e)}`, "bad", 8000))
      .finally(() => (loading = false));
  });

  const prefs = $derived(listPrefs.value);
  const metaOf = (id: number) => listMeta.byId.get(id);
  const dateOf = (id: number) => metaOf(id)?.dateModified ?? 0;
  const ratingOf = (id: number) => metaOf(id)?.rating ?? 0;

  const rows = $derived(sortMixes(filterMixes(mixes.list, metaOf, query, prefs.deepSearch), prefs.sortKey, prefs.asc, metaOf));
  const shownIds = $derived(rows.map((m) => m.id));

  // a search (or a deleted mix) that takes a ticked row off the screen un-ticks it, so a bulk
  // action never touches a mix the user cannot see
  $effect(() => {
    const kept = prune(selected, shownIds);
    if (kept.size !== selected.size) selected = kept;
  });

  const chosen = $derived(rows.filter((m) => selected.has(m.id)));
  const one = $derived(chosen.length === 1 ? chosen[0] : null);

  const columns = [
    { key: "sel", label: "" },
    { key: "name", label: "Name" },
    { key: "genre", label: "Genre" },
    { key: "play_time_secs", label: "Length", num: true },
    { key: "track_count", label: "Tracks", num: true },
    { key: "date", label: "Modified", num: true },
    { key: "rating", label: "Rating" },
  ];

  function sort(key: string) {
    if (key === "sel") return;
    const k = key as SortKey;
    if (prefs.sortKey === k) listPrefs.set({ asc: !prefs.asc });
    else listPrefs.set({ sortKey: k, asc: defaultAscending(k) });
  }

  const fmtDate = (ms: number) => (ms ? new Date(ms).toLocaleDateString(undefined, { year: "numeric", month: "short", day: "numeric" }) : "–");

  async function rate(m: MixSummary, r: number) {
    try {
      await listMeta.rate(m.id, r);
    } catch (e) {
      toast.show(`Could not set the rating: ${errText(e)}`, "bad", 7000);
    }
  }

  // Shift and Ctrl on a row click: the table reports only the row, so the modifiers are noted on
  // the way down (capture phase) and read by the row handler that follows.
  let mods = { shift: false, ctrl: false };
  const noteMods = (e: MouseEvent) => (mods = { shift: e.shiftKey, ctrl: e.ctrlKey || e.metaKey });

  function pickRow(m: MixSummary) {
    const r = clickRow({ selected, anchor }, shownIds, m.id, mods);
    selected = r.selected;
    anchor = r.anchor;
  }

  function tick(m: MixSummary) {
    const r = toggleRow({ selected, anchor }, m.id);
    selected = r.selected;
    anchor = r.anchor;
  }

  const allShown = $derived(rows.length > 0 && rows.every((m) => selected.has(m.id)));
  function toggleAll() {
    selected = allShown ? new Set() : new Set(shownIds);
  }

  function afterDuplicate(id: number) {
    selected = new Set([id]);
    anchor = id;
    query = "";
  }

  function exportChosen() {
    if (one) void exportPmk(one);
    else void exportMixes(chosen);
  }

  function onkey(e: KeyboardEvent) {
    if (mixes.doc || document.querySelector('[role="dialog"]')) return;
    const el = e.target as HTMLElement | null;
    if (el && (el.isContentEditable || ["INPUT", "TEXTAREA", "SELECT"].includes(el.tagName))) return;
    const mod = e.ctrlKey || e.metaKey;
    if (mod && e.key.toLowerCase() === "a") {
      e.preventDefault();
      selected = new Set(shownIds);
    } else if (!mod && e.key === "Delete" && chosen.length > 0) {
      e.preventDefault();
      deleteMixes(chosen);
    } else if (!mod && e.key === "Escape") {
      selected = new Set();
    } else if (!mod && e.key === "Enter" && one && !el?.closest("button, a")) {
      openMix(one.id);
    }
  }
</script>

<svelte:window onkeydown={onkey} />

<div class="list">
  <div class="bar">
    <h2>Mixes</h2>
    <span class="chip">{rows.length === mixes.list.length ? `${mixes.list.length} on this device` : `${rows.length} of ${mixes.list.length} shown`}</span>
    <Button variant="primary" onclick={newMix}>New mix</Button>
    <Button onclick={importPmk}>Import .pmk…</Button>
    <span class="sep"></span>
    <Button disabled={!one} onclick={() => one && openMix(one.id)}>Open</Button>
    <Button disabled={!one} onclick={() => one && renameMix(one)}>Rename…</Button>
    <Button disabled={!one} onclick={() => one && duplicateMix(one, afterDuplicate)} title="Make a copy of the saved mix under a new name">Duplicate…</Button>
    <Button disabled={chosen.length === 0} onclick={exportChosen} title="Write the saved mixes as .pmk files">
      {chosen.length > 1 ? `Export ${chosen.length}…` : "Export .pmk…"}
    </Button>
    <Button disabled={chosen.length === 0} onclick={() => deleteMixes(chosen)}>
      <span class:bad={chosen.length > 0}>{chosen.length > 1 ? `Delete ${chosen.length}…` : "Delete…"}</span>
    </Button>
  </div>

  <div class="bar find">
    <input
      type="search"
      class="search"
      placeholder={prefs.deepSearch ? "Search name, genre, producer, remixer, comments…" : "Search by name…"}
      aria-label="Search mixes"
      bind:value={query}
      onkeydown={(e) => {
        if (e.key === "Escape" && query) {
          e.stopPropagation();
          query = "";
        }
      }}
    />
    <label class="ctl">Sort
      <select value={prefs.sortKey} onchange={(e) => sort(e.currentTarget.value)} aria-label="Sort by">
        {#each Object.entries(SORT_LABELS) as [key, label] (key)}<option value={key}>{label}</option>{/each}
      </select>
    </label>
    <Button small onclick={() => listPrefs.set({ asc: !prefs.asc })} title={prefs.asc ? "Ascending: click for descending" : "Descending: click for ascending"} aria-label="Sort direction">
      {prefs.asc ? "▲" : "▼"}
    </Button>
    <span class="sep"></span>
    <label class="check"><input type="checkbox" checked={allShown} disabled={rows.length === 0} onchange={toggleAll} /> Select all shown</label>
    {#if chosen.length > 0}
      <span class="chip">{chosen.length} selected</span>
      <Button variant="link" onclick={() => (selected = new Set())}>Clear</Button>
    {:else}
      <span class="chip hint">Click a row to pick it; Ctrl or Shift for several.</span>
    {/if}
  </div>

  <details class="opts">
    <summary>List options</summary>
    <div class="optrow">
      <label class="check"
        ><input type="checkbox" checked={prefs.deepSearch} onchange={(e) => listPrefs.set({ deepSearch: e.currentTarget.checked })} /> Search also looks in genre, producer, remixer, comments and year</label
      >
    </div>
    <div class="optrow">
      <label class="ctl" for="ml-exist">When an exported file already exists</label>
      <select id="ml-exist" value={prefs.onExisting} onchange={(e) => listPrefs.set({ onExisting: e.currentTarget.value as "number" | "replace" | "skip" })}>
        <option value="number">Keep it and write a numbered name (never overwrite)</option>
        <option value="skip">Keep it and skip that mix</option>
        <option value="replace">Replace it (the confirmation says which)</option>
      </select>
    </div>
    <div class="muted small">Sort and these options are remembered on this computer. They do not change anything on the device.</div>
  </details>

  {#if mv.pending.length > 0}
    <div class="banner">
      Choose a mix to add {mv.pending.length} track{mv.pending.length === 1 ? "" : "s"} to, or create a new one.
      <Button variant="link" onclick={() => (mv.pending = [])}>Cancel</Button>
    </div>
  {/if}

  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="body" onclickcapture={noteMods}>
    {#if rows.length > 0}
      <Table
        {columns}
        {rows}
        rowKey={(m) => m.id}
        sortKey={prefs.sortKey}
        sortAsc={prefs.asc}
        onsort={sort}
        rowClass={(m) => (selected.has(m.id) ? "picked" : "")}
        onrowclick={pickRow}
      >
        {#snippet cell(m, col)}
          {#if col.key === "sel"}
            <input type="checkbox" checked={selected.has(m.id)} aria-label="Select “{m.name}”" onclick={(e) => (e.stopPropagation(), tick(m))} />
          {:else if col.key === "name"}
            <Button variant="link" onclick={(e) => (e.stopPropagation(), openMix(m.id))}>{m.name}</Button>
          {:else if col.key === "genre"}
            {metaOf(m.id)?.genre || "–"}
          {:else if col.key === "play_time_secs"}
            {mmss(m.play_time_secs)}
          {:else if col.key === "track_count"}
            {m.track_count}
          {:else if col.key === "date"}
            {fmtDate(dateOf(m.id))}
          {:else}
            <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
            <span onclick={(e) => e.stopPropagation()}>
              {#if listMeta.byId.has(m.id)}<Stars rating={ratingOf(m.id)} onchange={(r) => rate(m, r)} />{:else}<span class="muted">–</span>{/if}
            </span>
          {/if}
        {/snippet}
      </Table>
    {:else if mixes.list.length > 0}
      <div class="empty">
        <p>No mix matches “{query}”.</p>
        <Button onclick={() => (query = "")}>Clear the search</Button>
      </div>
    {:else if !loading}
      <div class="empty">
        <p>No mixes on this device yet.</p>
        <p class="muted">Create one, or import a .pmk file made with the Tonium Editor or the Pacemaker.</p>
        <Button variant="primary" onclick={newMix}>New mix</Button>
      </div>
    {/if}
  </div>
</div>

<style>
  .list {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
  }
  .bar {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 8px;
    padding: 10px 14px;
    border-bottom: 1px solid var(--line);
  }
  .bar.find {
    padding: 6px 14px;
  }
  h2 {
    margin: 0;
    font-size: 16px;
  }
  .chip {
    color: var(--muted);
    font-size: 12px;
    margin-right: 8px;
  }
  .sep {
    width: 1px;
    height: 20px;
    background: var(--line);
    margin: 0 4px;
  }
  .search {
    flex: 1 1 220px;
    min-width: 160px;
    max-width: 420px;
  }
  .ctl {
    display: flex;
    gap: 6px;
    align-items: center;
    color: var(--muted);
    font-size: 12px;
  }
  .check {
    display: flex;
    gap: 6px;
    align-items: center;
    color: var(--text);
    font-size: 13px;
  }
  .opts {
    border-bottom: 1px solid var(--line);
    padding: 4px 14px;
    font-size: 12px;
  }
  .opts summary {
    cursor: pointer;
    color: var(--muted);
  }
  .optrow {
    display: flex;
    gap: 8px;
    align-items: center;
    margin: 6px 0;
  }
  .small {
    margin: 4px 0 6px;
  }
  .banner {
    padding: 8px 14px;
    background: var(--accent-soft);
    border-bottom: 1px solid var(--line);
    color: var(--text);
    display: flex;
    gap: 12px;
    align-items: center;
  }
  .body {
    flex: 1;
    min-height: 0;
    overflow: auto;
  }
  .body :global(tr) {
    user-select: none;
  }
  .body :global(tr.picked) {
    background: var(--accent-soft) !important;
  }
  .empty {
    padding: 28px 14px;
  }
  .empty p {
    margin: 0 0 8px;
  }
  .muted {
    color: var(--muted);
  }
  .bad {
    color: var(--bad);
  }
</style>
