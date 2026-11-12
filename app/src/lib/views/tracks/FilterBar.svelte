<script lang="ts">
  // The toolbar above the table: genre, BPM range, key, analysed / missing, presets, search help
  // and the column chooser; under it, anything the search box could not read.
  import { filters } from "$lib/state/filters.svelte";
  import { gridBatch } from "$lib/state/gridBatch.svelte";
  import { suspects } from "$lib/state/gridSuspects.svelte";
  import Button from "$lib/ui/Button.svelte";
  import { COLUMNS } from "./columns";
  import { labelFilterActive } from "$lib/state/labels";
  import LabelFilterBar from "./LabelFilterBar.svelte";
  import PresetsMenu from "./PresetsMenu.svelte";
  import QueryHelp from "./QueryHelp.svelte";
  import { compileQuery } from "./query";
  import { ALL_CAMELOT, filterIsActive, type Camelot } from "./trackLogic";
  import { tracksState } from "./tracksState.svelte";

  let {
    genres,
    focusKey,
    shown,
    total,
    selected,
    suspectIds = [],
  }: {
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

  // terms of the search box that were skipped or taken as text, so a typo is not silent
  const diagnostics = $derived(compileQuery(filters.query).diagnostics);

  // the options of the key menu: whatever is active must stay listed even after the focus moves
  const compatOptions = $derived.by(() => {
    const codes = new Set<string>();
    if (focusKey) codes.add(focusKey.code);
    if (tracksState.keyFilter.startsWith("compat:")) codes.add(tracksState.keyFilter.slice(7));
    return [...codes];
  });

  const anyFilter = $derived(
    filterIsActive({ ...tracksState.filter }) || labelFilterActive(tracksState.labelFilter) || filters.onlyNeeds || !!filters.query || tracksState.onlySuspect,
  );

  // Analysed and Missing are opposites; pressing one releases the other, pressing it again clears it.
  function toggleAnalysed() {
    tracksState.onlyAnalysed = !tracksState.onlyAnalysed;
    if (tracksState.onlyAnalysed) filters.onlyNeeds = false;
  }
  function toggleMissing() {
    filters.onlyNeeds = !filters.onlyNeeds;
    if (filters.onlyNeeds) tracksState.onlyAnalysed = false;
  }
  const clearAll = () => tracksState.clearAllFilters();
  function toggleSuspect() {
    tracksState.onlySuspect = !tracksState.onlySuspect;
    if (tracksState.onlySuspect) void suspects.ensure();
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

<svelte:window onclick={() => (chooser = false)} />

<div class="bar">
  <select aria-label="Genre" bind:value={tracksState.genre}>
    <option value="">All genres</option>
    {#each genres as g (g)}<option value={g}>{g}</option>{/each}
  </select>

  <label class="muted" for="bpm-min">BPM</label>
  <input id="bpm-min" class="bpm" inputmode="decimal" placeholder="min" aria-label="Minimum BPM" bind:value={tracksState.bpmMin} />
  –
  <input class="bpm" inputmode="decimal" placeholder="max" aria-label="Maximum BPM" bind:value={tracksState.bpmMax} />

  <select aria-label="Key" bind:value={tracksState.keyFilter}>
    <option value="">Any key</option>
    {#each compatOptions as code (code)}<option value="compat:{code}">Compatible with {code}</option>{/each}
    <optgroup label="Exactly">
      {#each ALL_CAMELOT as k (k.code)}<option value="is:{k.code}">{k.code}</option>{/each}
    </optgroup>
  </select>

  <LabelFilterBar />

  <Button small active={tracksState.onlyAnalysed} onclick={toggleAnalysed} title="Only tracks with a beat grid">Analysed</Button>
  <Button small active={filters.onlyNeeds} onclick={toggleMissing} title="Only tracks whose beat grid (.str) is missing or unreadable">Missing</Button>
  <Button small active={tracksState.onlySuspect} onclick={toggleSuspect} title={suspectTitle}>
    Suspect grid{suspects.checking ? " …" : suspects.report ? ` (${suspects.count})` : ""}
  </Button>
  {#if tracksState.onlySuspect}
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
    {shown === total ? `${total} tracks` : `${shown} of ${total} tracks`}{selected > 1 ? ` · ${selected} selected` : ""}
  </span>
  <span class="legend muted" aria-hidden="true">
    <span><i class="low"></i>kick/bass</span><span><i class="mid"></i>mids</span><span><i class="high"></i>highs</span>
  </span>

  <PresetsMenu />
  <QueryHelp />

  <div class="chooser">
    <Button small active={chooser} onclick={(e) => (e.stopPropagation(), (chooser = !chooser))}>Columns</Button>
    {#if chooser}
      <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
      <div class="pop" onclick={(e) => e.stopPropagation()}>
        {#each tracksState.prefs.order.map((k) => COLUMNS.find((c) => c.key === k)!).filter((c) => !c.fixed) as c (c.key)}
          <label>
            <input type="checkbox" checked={tracksState.prefs.visible[c.key]} onchange={() => tracksState.toggleColumn(c.key)} />
            {c.label}
          </label>
        {/each}
        <p class="hint muted">Drag a column header to move it. Click a header to sort, shift-click to add a second key.</p>
        <Button small variant="link" onclick={() => tracksState.resetColumns()}>Reset columns</Button>
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
  .spacer {
    flex: 1;
  }
  .count {
    font-variant-numeric: tabular-nums;
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
    position: absolute;
    right: 0;
    top: calc(100% + 4px);
    z-index: 30;
    display: flex;
    flex-direction: column;
    gap: 4px;
    min-width: 140px;
    max-height: 70vh;
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
  .hint {
    margin: 4px 0 0;
    max-width: 200px;
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
