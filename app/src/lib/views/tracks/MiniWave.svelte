<script lang="ts">
  // The small colour waveform in a table row. Rows are only mounted while visible, and the
  // request waits a moment so rows that fly past during a fast scroll never ask for one.
  import type { Peak } from "$lib/api/analysis";
  import { sections } from "$lib/state/sections.svelte";
  import SectionBand from "../analysis/SectionBand.svelte";
  import { cachedPeaks, requestPeaks } from "./peakCache";
  import { peakTick } from "./peakTick.svelte";
  import { drawPeaks } from "./waveDraw";

  let { root, location, columns = 64, height = 18, trackId = null }: { root: string; location: string; columns?: number; height?: number; trackId?: number | null } = $props();

  // coloured section markers (analysis area) along the bottom edge, when the track has some
  const marks = $derived(trackId !== null && sections.showMarkers ? sections.of(trackId) : null);

  let canvas = $state<HTMLCanvasElement | null>(null);
  let width = $state(0);
  let peaks = $state.raw<Peak[] | null>(null);

  $effect(() => {
    // moves when the background analysis has just made this track's waveform
    peakTick.of(location);
    const cached = cachedPeaks(root, location, columns);
    if (cached !== undefined) {
      peaks = cached;
      return;
    }
    peaks = null;
    let cancel = () => {};
    const timer = setTimeout(() => (cancel = requestPeaks(root, location, columns, (p) => (peaks = p))), 120);
    return () => {
      clearTimeout(timer);
      cancel();
    };
  });

  $effect(() => {
    if (canvas && width > 0) drawPeaks(canvas, peaks, width, height);
  });
</script>

<div class="mini" bind:clientWidth={width} style:height="{height}px">
  <canvas bind:this={canvas} style:width="{width}px" style:height="{height}px" aria-hidden="true"></canvas>
  {#if marks}<div class="marks"><SectionBand sections={marks} /></div>{/if}
</div>

<style>
  .mini {
    position: relative;
    width: 100%;
  }
  .marks {
    position: absolute;
    left: 0;
    right: 0;
    bottom: 0;
    height: 4px;
  }
  canvas {
    display: block;
  }
</style>
