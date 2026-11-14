<script lang="ts">
  // The label part of the Tracks filter bar: colour swatches, a tag picker (any / all) with the
  // chosen tags shown as chips, and an energy range. The conditions live in
  // `table.labelFilter` and are applied by `filterByLabels` (state/labels.ts); these are
  // app-side labels, so tracks without them simply do not match a condition on them.
  import {
    ENERGY_LEVELS,
    LABEL_COLOURS,
    colourName,
    tagKey,
    type LabelColour,
  } from "$lib/state/labels";
  import { labelEditor, labels } from "$lib/state/labels.svelte";
  import Button from "$lib/ui/Button.svelte";
  import ColourSwatch from "./ColourSwatch.svelte";
  import type { TableState } from "./tableState.svelte";
  import { tracksState } from "./tracksState.svelte";

  let { table = tracksState }: { table?: TableState } = $props();

  const f = $derived(table.labelFilter);
  let open = $state(false);
  let find = $state("");
  let wrap = $state<HTMLElement | null>(null);
  // the popover opens to the left instead when the button is too near the window's right edge
  let alignRight = $state(false);

  function toggle(e: MouseEvent) {
    e.stopPropagation();
    open = !open;
    if (open && wrap) alignRight = wrap.getBoundingClientRect().left + 250 > innerWidth;
  }

  const vocab = $derived(labels.vocabulary.filter((v) => !find.trim() || tagKey(v.tag).includes(tagKey(find.trim()))));
  const chosen = $derived(new Set(f.tags.map(tagKey)));

  // a tag that no track carries any more (deleted, renamed or merged in the manager) would hide
  // every track with no way to see why, so it leaves the filter
  $effect(() => {
    if (!labels.root) return;
    const live = new Set(labels.vocabulary.map((v) => tagKey(v.tag)));
    if (f.tags.some((t) => !live.has(tagKey(t)))) table.labelFilter.tags = f.tags.filter((t) => live.has(tagKey(t)));
  });

  function toggleColour(c: LabelColour) {
    table.labelFilter.colours = f.colours.includes(c) ? f.colours.filter((x) => x !== c) : [...f.colours, c];
  }
  function toggleTag(tag: string) {
    table.labelFilter.tags = chosen.has(tagKey(tag)) ? f.tags.filter((t) => tagKey(t) !== tagKey(tag)) : [...f.tags, tag];
  }
  function setMin(v: string) {
    const n = v === "" ? null : Number(v);
    table.labelFilter.energyMin = n;
    // the range can't be upside down: pushing the minimum past the maximum drags the maximum along
    if (n !== null && f.energyMax !== null && f.energyMax < n) table.labelFilter.energyMax = n;
  }
  function setMax(v: string) {
    const n = v === "" ? null : Number(v);
    table.labelFilter.energyMax = n;
    if (n !== null && f.energyMin !== null && f.energyMin > n) table.labelFilter.energyMin = n;
  }
</script>

<svelte:window onclick={() => (open = false)} />

<span class="grp" role="group" aria-label="Colour filter">
  {#each LABEL_COLOURS as c (c)}
    {@const on = f.colours.includes(c)}
    <button type="button" class="sw" class:on aria-pressed={on} aria-label="Colour {colourName(c)}" title="{colourName(c)}{on ? ' (click to stop filtering by it)' : ''}" onclick={() => toggleColour(c)}>
      <ColourSwatch colour={c} size={12} />
    </button>
  {/each}
</span>

<span class="tagpick" bind:this={wrap}>
  <Button small active={f.tags.length > 0 || open} onclick={toggle} aria-haspopup="true" aria-expanded={open} title="Only tracks with these tags">
    Tags{f.tags.length ? ` (${f.tags.length})` : ""} ▾
  </Button>
  {#if open}
    <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
    <div class="pop" class:right={alignRight} onclick={(e) => e.stopPropagation()}>
      <div class="mode" role="group" aria-label="Match">
        <Button small active={f.tagMode === "any"} onclick={() => (table.labelFilter.tagMode = "any")} title="A track needs at least one of the chosen tags">Any</Button>
        <Button small active={f.tagMode === "all"} onclick={() => (table.labelFilter.tagMode = "all")} title="A track needs every chosen tag">All</Button>
        <span class="muted hint">of the chosen tags</span>
      </div>
      <input type="search" aria-label="Find a tag" placeholder="Find a tag" bind:value={find} />
      <ul>
        {#each vocab as v (tagKey(v.tag))}
          <li>
            <label>
              <input type="checkbox" checked={chosen.has(tagKey(v.tag))} onchange={() => toggleTag(v.tag)} />
              <span class="t">{v.tag}</span>
              <span class="muted n">{v.count}</span>
            </label>
          </li>
        {:else}
          <li class="muted">{labels.vocabulary.length ? "No tag matches." : "No tags yet."}</li>
        {/each}
      </ul>
      <div class="foot">
        <Button variant="link" onclick={() => ((open = false), labelEditor.manage())}>Manage tags…</Button>
        {#if f.tags.length}<Button variant="link" onclick={() => (table.labelFilter.tags = [])}>Clear tags</Button>{/if}
      </div>
    </div>
  {/if}
</span>

{#each f.tags as t (tagKey(t))}
  <span class="chip" title={f.tagMode === "all" ? "Tracks must have every chosen tag" : "Tracks must have at least one chosen tag"}>
    {t}
    <button type="button" aria-label="Stop filtering by the tag {t}" onclick={() => toggleTag(t)}>×</button>
  </span>
{/each}

<span class="grp energy">
  <label class="muted" for="en-min" title="Energy under the Energy source setting (your rating, the audio estimate, or yours else the estimate). Tracks with no energy never match an energy range.">Energy</label>
  <select id="en-min" aria-label="Lowest energy" value={f.energyMin === null ? "" : String(f.energyMin)} onchange={(e) => setMin(e.currentTarget.value)}>
    <option value="">any</option>
    {#each ENERGY_LEVELS as n (n)}<option value={String(n)}>{n}</option>{/each}
  </select>
  to
  <select aria-label="Highest energy" value={f.energyMax === null ? "" : String(f.energyMax)} onchange={(e) => setMax(e.currentTarget.value)}>
    <option value="">any</option>
    {#each ENERGY_LEVELS as n (n)}<option value={String(n)}>{n}</option>{/each}
  </select>
</span>

<style>
  .grp {
    display: inline-flex;
    align-items: center;
    gap: 3px;
  }
  .energy {
    gap: 6px;
  }
  .sw {
    display: inline-flex;
    padding: 3px;
    background: none;
    border: 1px solid transparent;
    border-radius: 4px;
    cursor: pointer;
  }
  .sw:hover {
    border-color: var(--line-hi);
  }
  .sw.on {
    border-color: var(--accent);
    background: var(--accent-soft);
  }
  .tagpick {
    position: relative;
  }
  .pop {
    position: absolute;
    left: 0;
    top: calc(100% + 4px);
    z-index: 30;
    width: 240px;
    padding: 8px 10px;
    background: var(--panel);
    border: 1px solid var(--line-hi);
    border-radius: 6px;
  }
  .pop.right {
    left: auto;
    right: 0;
  }
  .mode {
    display: flex;
    align-items: center;
    gap: 4px;
    margin-bottom: 6px;
  }
  .hint {
    font-size: 12px;
  }
  .pop input[type="search"] {
    width: 100%;
    box-sizing: border-box;
    margin-bottom: 6px;
  }
  .pop ul {
    max-height: 220px;
    overflow: auto;
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .pop label {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 2px 0;
    cursor: pointer;
  }
  .pop label input {
    padding: 0;
  }
  .t {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .n {
    font-size: 12px;
    font-variant-numeric: tabular-nums;
  }
  .foot {
    display: flex;
    justify-content: space-between;
    margin-top: 8px;
    font-size: 12px;
  }
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 1px 4px 1px 8px;
    background: var(--accent-soft);
    color: var(--accent);
    border-radius: 10px;
    font-size: 12px;
  }
  .chip button {
    width: 16px;
    height: 16px;
    padding: 0;
    background: none;
    border: none;
    border-radius: 8px;
    color: inherit;
    line-height: 1;
    cursor: pointer;
  }
  .chip button:hover {
    background: var(--hover);
  }
</style>
