<script lang="ts">
  // A strip of coloured section markers, positioned over a waveform by time: one block per
  // section, its colour from the kind, fainter the less sure the detection is. Used on the Tracks
  // table's mini waveform and the grid editor's overview. Purely visual: the parent decides where
  // it sits (it fills its positioned parent's width).
  import type { SectionOut } from "$lib/api/analysis";
  import { confidenceOpacity, coveredSecs, KIND_LABEL, kindColor, sectionTitle, summaryTitle } from "./sectionLogic";

  let {
    sections,
    duration = 0,
    labels = false,
    titles = false,
  }: {
    sections: readonly SectionOut[];
    /** Length the strip spans, seconds. Default: to the end of the last section. */
    duration?: number;
    /** Write the kind in each block that is wide enough. */
    labels?: boolean;
    /** Each block has its own tooltip (when the strip takes pointer events). */
    titles?: boolean;
  } = $props();

  const total = $derived(duration > 0 ? duration : coveredSecs(sections));
  const pct = (secs: number) => (total > 0 ? Math.min(100, Math.max(0, (secs / total) * 100)) : 0);
</script>

{#if sections.length > 0 && total > 0}
  <div class="band" role="img" aria-label={summaryTitle(sections)} title={titles ? undefined : summaryTitle(sections)}>
    {#each sections as s, i (i)}
      <i
        class="seg"
        class:first={i === 0}
        style:left="{pct(s.start_secs)}%"
        style:width="{Math.max(0, pct(s.end_secs) - pct(s.start_secs))}%"
        style:--c={kindColor(s.kind)}
        style:--o={confidenceOpacity(s.confidence)}
        title={titles ? sectionTitle(s) : undefined}
      >
        {#if labels}<span>{KIND_LABEL[s.kind]}</span>{/if}
      </i>
    {/each}
  </div>
{/if}

<style>
  .band {
    position: relative;
    width: 100%;
    height: 100%;
    overflow: hidden;
  }
  .seg {
    position: absolute;
    top: 0;
    bottom: 0;
    box-sizing: border-box;
    background: color-mix(in srgb, var(--c) calc(var(--o) * 100%), transparent);
    border-left: 1px solid var(--deep);
    overflow: hidden;
    white-space: nowrap;
    font-style: normal;
  }
  .seg.first {
    border-left: none;
  }
  .seg span {
    padding: 0 4px;
    font-size: 10px;
    line-height: 14px;
    font-weight: 600;
    color: var(--deep);
    text-overflow: clip;
  }
</style>
