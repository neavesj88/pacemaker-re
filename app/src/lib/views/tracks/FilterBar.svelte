<script lang="ts">
  // The toolbar above the table: genre, BPM range, key, analysed / missing, column chooser.
  import { filters } from "$lib/state/filters.svelte";
  import Button from "$lib/ui/Button.svelte";
  import { COLUMNS } from "./columns";
  import { ALL_CAMELOT, filterIsActive, type Camelot } from "./trackLogic";
  import { tracksState } from "./tracksState.svelte";

  let {
    genres,
    focusKey,
    shown,
    total,
    selected,
  }: {
    genres: string[];
    /** Camelot key of the track the inspector shows, for "compatible with ...". */
    focusKey: Camelot | null;
    shown: number;
    total: number;
    selected: number;
  } = $props();

  let chooser = $state(false);

  // the options of the key menu: whatever is active must stay listed even after the focus moves
  const compatOptions = $derived.by(() => {
    const codes = new Set<string>();
    if (focusKey) codes.add(focusKey.code);
    if (tracksState.keyFilter.startsWith("compat:")) codes.add(tracksState.keyFilter.slice(7));
    return [...codes];
  });

  const anyFilter = $derived(
    filterIsActive({ ...tracksState.filter }) || filters.onlyNeeds,
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
  function clearAll() {
    tracksState.clearFilters();
    filters.onlyNeeds = false;
  }

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

  <Button small active={tracksState.onlyAnalysed} onclick={toggleAnalysed} title="Only tracks with a beat grid">Analysed</Button>
  <Button small active={filters.onlyNeeds} onclick={toggleMissing} title="Only tracks whose beat grid (.str) is missing or unreadable">Missing</Button>
  {#if anyFilter}<Button small variant="link" onclick={clearAll}>Clear filters</Button>{/if}

  <span class="spacer"></span>
  <span class="muted count" aria-live="polite">
    {shown === total ? `${total} tracks` : `${shown} of ${total} tracks`}{selected > 1 ? ` · ${selected} selected` : ""}
  </span>
  <span class="legend muted" aria-hidden="true">
    <span><i class="low"></i>kick/bass</span><span><i class="mid"></i>mids</span><span><i class="high"></i>highs</span>
  </span>

  <div class="chooser">
    <Button small active={chooser} onclick={(e) => (e.stopPropagation(), (chooser = !chooser))}>Columns</Button>
    {#if chooser}
      <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
      <div class="pop" onclick={(e) => e.stopPropagation()}>
        {#each COLUMNS.filter((c) => !c.fixed) as c (c.key)}
          <label>
            <input type="checkbox" checked={tracksState.prefs.visible[c.key]} onchange={() => tracksState.toggleColumn(c.key)} />
            {c.label}
          </label>
        {/each}
      </div>
    {/if}
  </div>
</div>

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
</style>
