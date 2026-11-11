<script lang="ts">
  // The small colour waveform in a table row. Rows are only mounted while visible, and the
  // request waits a moment so rows that fly past during a fast scroll never ask for one.
  import type { Peak } from "$lib/api/analysis";
  import { cachedPeaks, requestPeaks } from "./peakCache";
  import { drawPeaks } from "./waveDraw";

  let { root, location, columns = 64, height = 18 }: { root: string; location: string; columns?: number; height?: number } = $props();

  let canvas = $state<HTMLCanvasElement | null>(null);
  let width = $state(0);
  let peaks = $state.raw<Peak[] | null>(null);

  $effect(() => {
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
</div>

<style>
  .mini {
    width: 100%;
  }
  canvas {
    display: block;
  }
</style>
