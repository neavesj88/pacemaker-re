<script lang="ts">
  import type { Peak } from "$lib/api/analysis";
  import { sections } from "$lib/state/sections.svelte";
  import SectionBand from "../analysis/SectionBand.svelte";
  import { bandsOver, clamp, isDownbeat } from "./gridMath";

  // The whole track in one strip: the visible window of the main waveform is the lit rectangle;
  // click or drag to move it. Bar lines are faint ticks so a drifting grid shows from afar.
  let {
    peaks,
    beatSecs,
    firstDownbeat,
    duration,
    cue,
    playhead,
    viewStart,
    viewSpan,
    oncentre,
    height = 72,
    trackId = null,
  }: {
    peaks: Peak[] | null;
    beatSecs: ArrayLike<number>;
    firstDownbeat: number | null;
    duration: number;
    cue: number | null;
    playhead: number;
    viewStart: number;
    viewSpan: number;
    oncentre: (secs: number) => void;
    height?: number;
    /** The track shown, for its section markers (analysis area). */
    trackId?: number | null;
  } = $props();

  const marks = $derived(trackId !== null && sections.showMarkers ? sections.of(trackId) : null);

  let canvas = $state<HTMLCanvasElement | null>(null);
  let host = $state<HTMLDivElement | null>(null);
  let width = $state(0);

  const css = (name: string) => (host ? getComputedStyle(host).getPropertyValue(name).trim() || "#888" : "#888");

  $effect(() => {
    const c = canvas;
    const W = width;
    if (!c || W === 0) return;
    const dpr = window.devicePixelRatio || 1;
    c.width = Math.round(W * dpr);
    c.height = Math.round(height * dpr);
    const g = c.getContext("2d");
    if (!g) return;
    g.scale(dpr, dpr);
    g.fillStyle = css("--deep");
    g.fillRect(0, 0, W, height);
    const mid = height / 2;
    const half = height / 2 - 2;
    if (peaks && peaks.length > 0) {
      const cols = [css("--g-low"), css("--g-mid"), css("--g-high")];
      const scale = [1, 0.75, 0.45];
      for (let x = 0; x < W; x++) {
        const b = bandsOver(peaks, x / W, (x + 1) / W);
        for (let k = 0; k < 3; k++) {
          const h = (b[k] / 255) * half * scale[k];
          if (h <= 0) continue;
          g.fillStyle = cols[k];
          g.fillRect(x, mid - h, 1, h * 2);
        }
      }
    }
    if (duration > 0) {
      // bar lines, only when they are at least 3 px apart
      if (beatSecs.length > 4 && ((beatSecs[4] - beatSecs[0]) / duration) * W >= 3) {
        g.globalAlpha = 0.35;
        g.fillStyle = css("--text");
        for (let i = 0; i < beatSecs.length; i++) {
          if (isDownbeat(i, firstDownbeat)) g.fillRect(Math.round((beatSecs[i] / duration) * W), 0, 1, height);
        }
        g.globalAlpha = 1;
      }
      if (cue !== null) {
        g.fillStyle = css("--g-cue");
        g.fillRect(Math.round((cue / duration) * W), 0, 2, height);
      }
      // the visible window
      const x0 = (viewStart / duration) * W;
      const w = Math.max(3, (viewSpan / duration) * W);
      g.fillStyle = css("--accent-soft");
      g.fillRect(x0, 0, w, height);
      g.strokeStyle = css("--accent");
      g.lineWidth = 1;
      g.strokeRect(x0 + 0.5, 0.5, w - 1, height - 1);
      g.fillStyle = css("--text");
      g.fillRect(Math.round((playhead / duration) * W) - 1, 0, 2, height);
    }
  });

  let dragging = false;

  function at(e: PointerEvent) {
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    oncentre(clamp(((e.clientX - r.left) / r.width) * duration, 0, duration));
  }
</script>

<div class="ov" bind:this={host} bind:clientWidth={width} style:height="{height}px">
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <canvas
    bind:this={canvas}
    style:width="{width}px"
    style:height="{height}px"
    aria-label="Overview of the whole track. Click or drag to move the visible window."
    onpointerdown={(e) => {
      (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
      dragging = true;
      at(e);
    }}
    onpointermove={(e) => dragging && at(e)}
    onpointerup={() => (dragging = false)}
    onpointercancel={() => (dragging = false)}
  ></canvas>
  {#if marks}<div class="marks"><SectionBand sections={marks} {duration} labels /></div>{/if}
</div>

<style>
  .ov {
    position: relative;
    width: 100%;
    background: var(--deep);
    border: 1px solid var(--line);
    border-radius: 4px;
    overflow: hidden;
    margin-top: 8px;
  }
  /* over the top of the strip; clicks fall through to the canvas, which moves the window */
  .marks {
    position: absolute;
    top: 0;
    left: 0;
    right: 0;
    height: 14px;
    pointer-events: none;
  }
  canvas {
    display: block;
    cursor: pointer;
    touch-action: none;
  }
</style>
