<script lang="ts">
  // Section markers over one clip's waveform: a coloured band along the top (one block per
  // detected section, fainter the less sure the detection is), a faint line down each boundary and
  // the kind written in blocks wide enough for it. Positions go through the clip's tempo curve
  // (`clipSectionSpans`), so a stretched clip's markers sit where its audio is heard. A clip whose
  // track has no sections draws nothing. Purely visual: it takes no pointer events.
  import type { Clip } from "$lib/api/mixes";
  import { clipSectionSpans } from "$lib/state/mixedit";
  import { sections } from "$lib/state/sections.svelte";
  import { confidenceOpacity, KIND_LABEL, kindColor } from "$lib/views/analysis/sectionLogic";
  import { SR } from "./layout";

  let { clip, pps }: { clip: Clip; pps: number } = $props();

  /** Narrowest block that gets its kind written in it, pixels. */
  const LABEL_MIN_PX = 54;

  const blocks = $derived.by(() => {
    const spans = clipSectionSpans(clip, sections.of(clip.trackId));
    const px = (local: number) => ((local - clip.trimStart) / SR) * pps;
    const w = ((clip.visibleEnd - clip.visibleStart) / SR) * pps;
    return spans
      .map((s) => ({ ...s, x0: px(s.from), x1: px(s.to) }))
      // only what overlaps the audible part of the clip, cut to it
      .filter((s) => s.x1 > 0 && s.x0 < w)
      .map((s) => ({ kind: s.kind, confidence: s.confidence, left: Math.max(0, s.x0), width: Math.min(w, s.x1) - Math.max(0, s.x0), startsHere: s.x0 >= 0 }));
  });
</script>

{#if blocks.length > 0}
  <div class="secs" aria-hidden="true">
    {#each blocks as b, i (i)}
      <i class="seg" class:cut={b.startsHere && i > 0} style:left="{b.left}px" style:width="{b.width}px" style:--c={kindColor(b.kind)} style:--o={confidenceOpacity(b.confidence)}>
        {#if b.width >= LABEL_MIN_PX}<span>{KIND_LABEL[b.kind]}</span>{/if}
      </i>
    {/each}
  </div>
{/if}

<style>
  .secs {
    position: absolute;
    inset: 0;
    pointer-events: none;
    overflow: hidden;
    z-index: 1;
  }
  .seg {
    position: absolute;
    top: 0;
    bottom: 0;
    box-sizing: border-box;
    font-style: normal;
    overflow: hidden;
    white-space: nowrap;
  }
  /* the band */
  .seg::before {
    content: "";
    position: absolute;
    left: 0;
    right: 0;
    top: 0;
    height: 4px;
    background: color-mix(in srgb, var(--c) calc(var(--o) * 100%), transparent);
  }
  /* the boundary line down the clip */
  .seg.cut {
    border-left: 1px dashed color-mix(in srgb, var(--c) 70%, transparent);
  }
  .seg span {
    position: absolute;
    left: 4px;
    top: 17px;
    font-size: 9px;
    line-height: 11px;
    font-weight: 600;
    color: var(--c);
    text-shadow: 0 0 3px var(--deep), 0 0 3px var(--deep);
  }
</style>
