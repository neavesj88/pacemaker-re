<script lang="ts">
  // A case's or mix's colour label and tags, small, for a list row or a header. Renders nothing
  // when it has none. Clicking a tag asks the list to filter by it.
  import { tagKey, type TrackLabels } from "$lib/state/labels";
  import ColourSwatch from "../tracks/ColourSwatch.svelte";

  let { labels, ontag, max = 4 }: { labels: TrackLabels | undefined; ontag?: (tag: string) => void; max?: number } = $props();
  const shown = $derived((labels?.tags ?? []).slice(0, max));
  const more = $derived(Math.max(0, (labels?.tags.length ?? 0) - max));
</script>

{#if labels && (labels.colour || labels.tags.length)}
  <span class="things">
    {#if labels.colour}<ColourSwatch colour={labels.colour} size={11} />{/if}
    {#each shown as t (tagKey(t))}
      {#if ontag}
        <button type="button" class="tag" title="Only show those tagged “{t}”" onclick={(e) => (e.stopPropagation(), ontag(t))}>{t}</button>
      {:else}
        <span class="tag">{t}</span>
      {/if}
    {/each}
    {#if more > 0}<span class="more" title={labels.tags.slice(max).join(", ")}>+{more}</span>{/if}
  </span>
{/if}

<style>
  .things {
    display: inline-flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 4px;
    margin-left: 6px;
    vertical-align: middle;
  }
  .tag {
    padding: 0 6px;
    background: var(--deep);
    border: 1px solid var(--line);
    border-radius: 8px;
    color: var(--text);
    font: inherit;
    font-size: 11px;
    line-height: 16px;
  }
  button.tag {
    cursor: pointer;
  }
  button.tag:hover {
    border-color: var(--line-hi);
  }
  .more {
    color: var(--muted);
    font-size: 11px;
  }
</style>
