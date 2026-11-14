<script lang="ts">
  // The toolbar above the table: genre, BPM range, key, analysed / missing, presets, search help
  // and the column chooser; under it, anything the search box could not read.
  import { caseMembers } from "$lib/state/caseMembers.svelte";
  import { gridBatch } from "$lib/state/gridBatch.svelte";
  import { suspects } from "$lib/state/gridSuspects.svelte";
  import Button from "$lib/ui/Button.svelte";
  import { CASE_ONLY, COLUMNS } from "./columns";
  import { popoverPlace, type PopoverPlace } from "./popoverPlace";
  import { labelFilterActive } from "$lib/state/labels";
  import LabelFilterBar from "./LabelFilterBar.svelte";
  import PresetsMenu from "./PresetsMenu.svelte";
  import QueryHelp from "./QueryHelp.svelte";
  import { compileQuery } from "./query";
  import { ALL_CAMELOT, filterIsActive, type Camelot } from "./trackLogic";
  import { session } from "$lib/state/session.svelte";
  import type { TableState } from "./tableState.svelte";
  import { tracksState } from "./tracksState.svelte";

  let {
    genres,
    focusKey,
    shown,
    total,
    selected,
    suspectIds = [],
    table = tracksState,
    ownSearch = false,
    withPresets = true,
  }: {
    /** This bar's table: its own filters, search and column choice. The Tracks view's by default. */
    table?: TableState;
    /** Draws a search box in the bar (the Tracks view's search is the app header's). */
    ownSearch?: boolean;
    /** The saved-filter presets and the search help belong to the Tracks view. */
    withPresets?: boolean;
    /** Flagged tracks among the rows now shown: what "Review" opens the batch grid tools for. */
    suspectIds?: number[];
    genres: string[];
    /** Camelot key of the track the inspector shows, for "compatible with ...". */
    focusKey: Camelot | null;
    shown: number;
    total: number;
    selected: number;
  } = $props();

  let chooser = $state(false);
  /** Where the column list sits: under its button, kept inside the window (see popoverPlace). */
  let place = $state<PopoverPlace | null>(null);
  function toggleChooser(e: MouseEvent) {
    e.stopPropagation();
    chooser = !chooser;
    if (chooser) place = popoverPlace(e.currentTarget as HTMLElement, 260);
  }

  /** Every case, by name; the chosen one stays listed even if the case list is re-read. */
  const caseOptions = $derived([...(session.library?.cases ?? [])].sort((a, b) => a.name.localeCompare(b.name, undefined, { sensitivity: "base" })));
  /** What the option says: the case's size, and how many of its tracks are not in this library
   *  (the table lists only the ones it has) once the members are read. */
  function caseLabel(c: { id: number; name: string; track_count: number }): string {
    const ids = caseMembers.tracksOf(c.id);
    const have = session.library ? libraryIds : null;
    if (!ids || !have) return `${c.name} (${c.track_count})`;
    const missing = ids.filter((id) => !have.has(id)).length;
    return missing === 0 ? `${c.name} (${ids.length})` : `${c.name} (${ids.length - missing}, ${missing} missing)`;
  }
  const libraryIds = $derived(new Set((session.library?.tracks ?? []).map((t) => t.track_id)));
  $effect(() => {
    if (session.library) void caseMembers.ensure();
  });
  // a chosen case that was deleted would silently hide everything
  $effect(() => {
    if (table.caseId !== null && session.library && !session.library.cases.some((c) => c.id === table.caseId)) table.setCase(null);
  });

  // terms of the search box that were skipped or taken as text, so a typo is not silent
  const diagnostics = $derived(compileQuery(table.search.query).diagnostics);

  // the options of the key menu: whatever is active must stay listed even after the focus moves
  const compatOptions = $derived.by(() => {
    const codes = new Set<string>();
    if (focusKey) codes.add(focusKey.code);
    if (table.keyFilter.startsWith("compat:")) codes.add(table.keyFilter.slice(7));
    return [...codes];
  });

  const anyFilter = $derived(
    filterIsActive({ ...table.filter }) || labelFilterActive(table.labelFilter) || table.search.onlyNeeds || !!table.search.query || table.onlySuspect || table.caseId !== null,
  );

  // Analysed and Missing are opposites; pressing one releases the other, pressing it again clears it.
  function toggleAnalysed() {
    table.onlyAnalysed = !table.onlyAnalysed;
    if (table.onlyAnalysed) table.search.onlyNeeds = false;
  }
  function toggleMissing() {
    table.search.onlyNeeds = !table.search.onlyNeeds;
    if (table.search.onlyNeeds) table.onlyAnalysed = false;
  }
  const clearAll = () => table.clearAllFilters();
  function toggleSuspect() {
    table.onlySuspect = !table.onlySuspect;
    if (table.onlySuspect) void suspects.ensure();
  }
  const suspectTitle = $derived(
    suspects.checking
      ? "Checking the beat grids…"
      : suspects.error
        ? suspects.error
        : suspects.report
          ? `Tracks whose tempo looks like half or double time for their genre, or whose beat grid looks poorly analysed (${suspects.report.checked} grids checked)`
          : "Check the beat grids for tracks that look half or double tempo, or poorly analysed",
  );
</script>

<svelte:window onclick={() => (chooser = false)} onresize={() => (chooser = false)} onkeydown={(e) => e.key === "Escape" && (chooser = false)} />

<div class="bar">
  {#if ownSearch}
    <input type="search" class="find" placeholder="Search title, artist, key, BPM…" aria-label="Search the library" bind:value={table.search.query} />
  {/if}
  <select aria-label="Genre" bind:value={table.genre}>
    <option value="">All genres</option>
    {#each genres as g (g)}<option value={g}>{g}</option>{/each}
  </select>

  <label class="muted" for="bpm-min">BPM</label>
  <input id="bpm-min" class="bpm" inputmode="decimal" placeholder="min" aria-label="Minimum BPM" bind:value={table.bpmMin} />
  –
  <input class="bpm" inputmode="decimal" placeholder="max" aria-label="Maximum BPM" bind:value={table.bpmMax} />

  <select aria-label="Key" bind:value={table.keyFilter}>
    <option value="">Any key</option>
    <!-- never shown: it keeps the menu as wide as its longest entry, so choosing a track (which adds
         "Compatible with ..." for its key) does not widen the menu and re-wrap the toolbar -->
    <option value="__width" disabled hidden>Compatible with 12B</option>
    {#each compatOptions as code (code)}<option value="compat:{code}">Compatible with {code}</option>{/each}
    <optgroup label="Exactly">
      {#each ALL_CAMELOT as k (k.code)}<option value="is:{k.code}">{k.code}</option>{/each}
    </optgroup>
  </select>

  <select aria-label="Case" title="Only the tracks of one case (crate) that exists on the device, in the case's own order when the column Case # is the sort. A smart case is listed here once it is on the device; the count is the case's size, the table can show fewer when some tracks are not in this library." value={table.caseId ?? ""} onchange={(e) => { const v = e.currentTarget.value; table.setCase(v === "" ? null : Number(v)); if (v !== "") void caseMembers.ensure(); }}>
    <option value="">All cases</option>
    {#each caseOptions as c (c.id)}<option value={c.id}>{caseLabel(c)}</option>{/each}
  </select>

  <LabelFilterBar {table} />

  <Button small active={table.onlyAnalysed} onclick={toggleAnalysed} title="Only tracks with a beat grid">Analysed</Button>
  <Button small active={table.search.onlyNeeds} onclick={toggleMissing} title="Only tracks whose beat grid (.str) is missing or unreadable">Missing</Button>
  <Button small active={table.onlySuspect} onclick={toggleSuspect} title={suspectTitle}>
    Suspect grid{suspects.checking ? " …" : suspects.report ? ` (${suspects.count})` : ""}
  </Button>
  {#if table.onlySuspect}
    <Button
      small
      disabled={suspectIds.length === 0}
      onclick={() => gridBatch.open(suspectIds, "suggested")}
      title="Preview the half / double fix for the {suspectIds.length} suspect tracks shown; nothing is changed until you confirm"
    >Review {suspectIds.length}…</Button>
    <Button small variant="ghost" onclick={() => (suspects.editing = true)} title="The genre tempo windows and the confidence floor">Rules…</Button>
  {/if}
  {#if anyFilter}<Button small variant="link" onclick={clearAll}>Clear filters</Button>{/if}

  <span class="spacer"></span>
  <span class="muted count" aria-live="polite">
    {shown === total ? `${total} tracks` : `${shown} of ${total} tracks`}
  </span>
  <!-- The waveform colour key and the selection count share one slot, so picking rows swaps one for the
       other in the same space: the toolbar never gets wider or taller (and the table never jumps)
       when the selection changes. Both are always laid out; the one not shown is hidden. -->
  <span class="slot">
    <span class="legend muted" class:off={selected > 1} aria-hidden="true">
      <span><i class="low"></i>kick/bass</span><span><i class="mid"></i>mids</span><span><i class="high"></i>highs</span>
    </span>
    <span class="muted picked" class:off={selected <= 1} aria-live="polite">{selected > 1 ? `${selected} selected` : ""}</span>
  </span>

  {#if withPresets}
    <PresetsMenu />
  {/if}
  <QueryHelp />

  <div class="chooser">
    <Button small active={chooser} aria-expanded={chooser} onclick={toggleChooser}>Columns</Button>
    {#if chooser && place}
      <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
      <div class="pop" role="dialog" tabindex="-1" aria-label="Columns" style:left="{place.left}px" style:top="{place.top}px" style:width="{place.width}px" style:max-height="{place.maxHeight}px" onclick={(e) => e.stopPropagation()}>
        {#each table.prefs.order.map((k) => COLUMNS.find((c) => c.key === k)!).filter((c) => !c.fixed) as c (c.key)}
          <label title={CASE_ONLY.includes(c.key) ? "Shown only while a case is chosen in the filter bar; it numbers the tracks in the case's order" : undefined}>
            <input type="checkbox" checked={table.prefs.visible[c.key]} onchange={() => table.toggleColumn(c.key)} />
            {c.label}{CASE_ONLY.includes(c.key) ? " (while a case is chosen)" : ""}
          </label>
        {/each}
        <div class="opts muted">Options</div>
        <label title="On a wide window Title and Artist share the spare room; their widths are the least they get. A column you drag keeps the width you gave it (Reset columns undoes that). Off: every column keeps its width."><input type="checkbox" checked={table.prefs.fillWidth} onchange={(e) => table.setFillWidth(e.currentTarget.checked)} /> Title and Artist fill a wide window</label>
        <label><input type="checkbox" checked={table.prefs.caseOrderOnPick} onchange={(e) => table.setCaseOrderOnPick(e.currentTarget.checked)} /> Choosing a case sorts by case order</label>
        <p class="hint muted">Drag a column header to move it. Click a header to sort, shift-click to add a second key.</p>
        <Button small variant="link" onclick={() => table.resetColumns()}>Reset columns</Button>
      </div>
    {/if}
  </div>
</div>
{#if diagnostics.length > 0}
  <ul class="diag" aria-live="polite">
    {#each diagnostics as d (d.term + d.message)}<li><code>{d.term}</code> {d.message}</li>{/each}
  </ul>
{/if}

<style>
  .bar {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 8px;
    padding: 7px 12px;
    border-bottom: 1px solid var(--line);
  }
  .bpm {
    width: 56px;
  }
  .find {
    width: 220px;
    max-width: 100%;
  }
  .spacer {
    flex: 1;
  }
  .count {
    font-variant-numeric: tabular-nums;
  }
  .slot {
    display: inline-grid;
    white-space: nowrap;
    font-size: 12px;
    font-variant-numeric: tabular-nums;
  }
  .slot > * {
    grid-area: 1 / 1;
  }
  .slot .off {
    visibility: hidden;
  }
  .picked {
    text-align: right;
  }
  .legend {
    display: inline-flex;
    gap: 10px;
    font-size: 12px;
  }
  .legend span {
    display: inline-flex;
    align-items: center;
    gap: 4px;
  }
  .legend i {
    width: 9px;
    height: 9px;
    border-radius: 2px;
  }
  .low {
    background: var(--accent);
  }
  .mid {
    background: color-mix(in srgb, var(--accent) 55%, white);
  }
  .high {
    background: var(--green);
  }
  .chooser {
    position: relative;
  }
  .pop {
    position: fixed;
    z-index: 30;
    box-sizing: border-box;
    display: flex;
    flex-direction: column;
    gap: 4px;
    overflow-y: auto;
    padding: 8px 10px;
    background: var(--panel);
    border: 1px solid var(--line-hi);
    border-radius: 6px;
  }
  .pop label {
    display: flex;
    align-items: center;
    gap: 8px;
    cursor: pointer;
  }
  .pop input {
    padding: 0;
  }
  .opts {
    margin-top: 4px;
    padding-top: 6px;
    border-top: 1px solid var(--line);
    font-size: 11px;
    letter-spacing: 0.06em;
    text-transform: uppercase;
  }
  .hint {
    margin: 4px 0 0;
    font-size: 11px;
    line-height: 1.4;
  }
  .diag {
    margin: 0;
    padding: 4px 12px;
    list-style: none;
    border-bottom: 1px solid var(--line);
    background: var(--accent-soft);
    font-size: 12px;
  }
  .diag code {
    color: var(--accent);
    font-family: ui-monospace, "Cascadia Mono", Consolas, monospace;
  }
</style>
