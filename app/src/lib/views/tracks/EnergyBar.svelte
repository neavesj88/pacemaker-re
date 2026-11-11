<script lang="ts">
  // An energy rating (1 to 10) as a short bar and the number. Low energy leans green, high leans
  // magenta, both from the palette tokens. Renders nothing when there is no rating.
  import { MAX_ENERGY } from "$lib/state/labels";

  let { energy }: { energy: number | null } = $props();
  const hot = $derived(energy === null ? 0 : Math.round(((energy - 1) / (MAX_ENERGY - 1)) * 100));
</script>

{#if energy !== null}
  <span class="en" title="Energy {energy} of {MAX_ENERGY}" role="img" aria-label="Energy {energy} of {MAX_ENERGY}">
    <span class="bar"><i style:width="{energy * 10}%" style:background="color-mix(in srgb, var(--accent) {hot}%, var(--green))"></i></span>
    <b>{energy}</b>
  </span>
{/if}

<style>
  .en {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
  }
  .bar {
    flex: none;
    width: 40px;
    height: 6px;
    border-radius: 3px;
    background: var(--line);
    overflow: hidden;
  }
  i {
    display: block;
    height: 100%;
  }
  b {
    font-weight: 600;
    font-variant-numeric: tabular-nums;
    min-width: 1.4em;
  }
</style>
