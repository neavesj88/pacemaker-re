<script lang="ts">
  // Filter a list of cases or mixes by colour label and tag: colour swatches, and a tag picker
  // (any / all) whose chosen tags show as chips. Used by the case list and the mix list; the
  // parent owns the filter and applies it with `matchesLabels` (state/labels.ts). Things with no
  // labels never match a condition on them.
  import { LABEL_COLOURS, colourName, tagKey, thingsWord, type LabelColour, type LabelFilter, type TagCount } from "$lib/state/labels";
  import Button from "$lib/ui/Button.svelte";
  import ColourSwatch from "../tracks/ColourSwatch.svelte";

  let { filter = $bindable(), vocabulary, thing }: { filter: LabelFilter; vocabulary: TagCount[]; thing: string } = $props();

  let open = $state(false);
  let find = $state("");
  const chosen = $derived(new Set(filter.tags.map(tagKey)));
  const vocab = $derived(vocabulary.filter((v) => !find.trim() || tagKey(v.tag).includes(tagKey(find.trim()))));

  // a tag that no longer exists (renamed away, or its last holder deleted) would hide everything
  // with no way to see why, so it leaves the filter
  $effect(() => {
    const live = new Set(vocabulary.map((v) => tagKey(v.tag)));
    if (filter.tags.some((t) => !live.has(tagKey(t)))) filter.tags = filter.tags.filter((t) => live.has(tagKey(t)));
  });

  function toggleColour(c: LabelColour) {
    filter.colours = filter.colours.includes(c) ? filter.colours.filter((x) => x !== c) : [...filter.colours, c];
  }
  function toggleTag(tag: string) {
    filter.tags = chosen.has(tagKey(tag)) ? filter.tags.filter((t) => tagKey(t) !== tagKey(tag)) : [...filter.tags, tag];
  }
</script>

<svelte:window onclick={() => (open = false)} />

<span class="bar">
  <span class="grp" role="group" aria-label="Colour filter">
    {#each LABEL_COLOURS as c (c)}
      {@const on = filter.colours.includes(c)}
      <button type="button" class="sw" class:on aria-pressed={on} aria-label="Colour {colourName(c)}" title="{colourName(c)}{on ? ' (click to stop filtering by it)' : ''}" onclick={() => toggleColour(c)}>
        <ColourSwatch colour={c} size={12} />
      </button>
    {/each}
  </span>

  <span class="tagpick">
    <Button
      small
      active={filter.tags.length > 0 || open}
      onclick={(e) => (e.stopPropagation(), (open = !open))}
      aria-haspopup="true"
      aria-expanded={open}
      title="Only {thingsWord(thing)} with these tags"
    >Tags{filter.tags.length ? ` (${filter.tags.length})` : ""} ▾</Button>
    {#if open}
      <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
      <div class="pop" onclick={(e) => e.stopPropagation()}>
        <div class="mode" role="group" aria-label="Match">
          <Button small active={filter.tagMode === "any"} onclick={() => (filter.tagMode = "any")} title="A {thing} needs at least one of the chosen tags">Any</Button>
          <Button small active={filter.tagMode === "all"} onclick={() => (filter.tagMode = "all")} title="A {thing} needs every chosen tag">All</Button>
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
            <li class="muted">{vocabulary.length ? "No tag matches." : `No ${thing} has a tag yet.`}</li>
          {/each}
        </ul>
        {#if filter.tags.length}
          <div class="foot"><Button variant="link" onclick={() => (filter.tags = [])}>Clear tags</Button></div>
        {/if}
      </div>
    {/if}
  </span>

  {#each filter.tags as t (tagKey(t))}
    <span class="chip">
      {t}
      <button type="button" aria-label="Stop filtering by the tag {t}" onclick={() => toggleTag(t)}>×</button>
    </span>
  {/each}
</span>

<style>
  .bar {
    display: inline-flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 4px;
  }
  .grp {
    display: inline-flex;
    align-items: center;
    gap: 3px;
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
