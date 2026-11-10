<script lang="ts">
  // Canvas waveform: peaks are 0..1 amplitudes, one per column group. Optional playhead
  // (progress 0..1) and markers (positions 0..1, e.g. beat or cue lines). Click to seek.
  let {
    peaks,
    progress = null,
    markers = [],
    height = 64,
    onseek,
  }: {
    peaks: number[] | null;
    progress?: number | null;
    markers?: number[];
    height?: number;
    onseek?: (position: number) => void;
  } = $props();

  let canvas = $state<HTMLCanvasElement | null>(null);
  let width = $state(0);

  const css = (name: string) => (canvas ? getComputedStyle(canvas).getPropertyValue(name).trim() : "#888");

  $effect(() => {
    if (!canvas || width === 0) return;
    const dpr = window.devicePixelRatio || 1;
    canvas.width = Math.round(width * dpr);
    canvas.height = Math.round(height * dpr);
    const g = canvas.getContext("2d");
    if (!g) return;
    g.scale(dpr, dpr);
    g.clearRect(0, 0, width, height);
    const mid = height / 2;
    if (peaks && peaks.length > 0) {
      g.fillStyle = css("--accent");
      const step = peaks.length / width;
      for (let x = 0; x < width; x++) {
        const p = peaks[Math.min(peaks.length - 1, Math.floor(x * step))] ?? 0;
        const h = Math.max(1, p * (height - 2));
        g.fillRect(x, mid - h / 2, 1, h);
      }
    } else {
      g.fillStyle = css("--line");
      g.fillRect(0, mid, width, 1);
    }
    g.fillStyle = css("--green");
    for (const m of markers) g.fillRect(Math.round(m * width), 0, 1, height);
    if (progress !== null) {
      g.fillStyle = css("--text");
      g.fillRect(Math.round(progress * width), 0, 2, height);
    }
  });

  function seek(e: MouseEvent) {
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    onseek?.(Math.min(1, Math.max(0, (e.clientX - r.left) / r.width)));
  }
</script>

<div class="wave" bind:clientWidth={width} style:height="{height}px">
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions -->
  <canvas bind:this={canvas} style:width="{width}px" style:height="{height}px" onclick={seek} aria-label="Waveform"></canvas>
</div>

<style>
  .wave {
    width: 100%;
    background: var(--deep);
    border: 1px solid var(--line);
    border-radius: 4px;
    overflow: hidden;
  }
  canvas {
    display: block;
  }
</style>
