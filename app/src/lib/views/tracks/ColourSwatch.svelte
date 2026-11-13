<script lang="ts">
  // A colour label as a small rounded square. Outlined when there is no colour and `empty` is
  // set; renders nothing otherwise.
  import { colourName, colourVar, type LabelColour } from "$lib/state/labels";

  let { colour, size = 12, empty = false }: { colour: LabelColour | null; size?: number; empty?: boolean } = $props();
</script>

{#if colour}
  <span class="sw" style:background={colourVar(colour)} style:width="{size}px" style:height="{size}px" title={colourName(colour)} role="img" aria-label="Colour {colourName(colour)}"></span>
{:else if empty}
  <span class="sw none" style:width="{size}px" style:height="{size}px" title="No colour" role="img" aria-label="No colour"></span>
{/if}

<style>
  .sw {
    display: inline-block;
    flex: none;
    border-radius: 3px;
    box-sizing: border-box;
  }
  .none {
    border: 1px dashed var(--line-hi);
  }
</style>
