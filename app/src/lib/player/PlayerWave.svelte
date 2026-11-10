<script lang="ts">
  // The player's colour waveform: three-band peaks (low = pink, mid = lilac, high = green),
  // beat ticks with the bar starts brighter, the cue flag, the beat loop and the playhead.
  // Click or drag to seek. Copied from ui/Waveform (single colour, integration-owned) rather
  // than extended, as ARCHITECTURE.md rule 7 asks.
  import type { Peak } from "$lib/api/analysis";

  let {
    peaks,
    beats = [],
    firstDownbeat = null,
    duration,
    progress,
    cue = null,
    loop = null,
    busy = false,
    disabled = false,
    onseek,
  }: {
    peaks: Peak[] | null;
    beats?: number[];
    firstDownbeat?: number | null;
    /** Seconds the full width stands for; ticks and markers are in the same unit. */
    duration: number;
    /** Playhead, 0..1. */
    progress: number;
    cue?: number | null;
    loop?: { start: number; end: number } | null;
    busy?: boolean;
    disabled?: boolean;
    onseek?: (fraction: number) => void;
  } = $props();

  const HEIGHT = 48;
  let canvas = $state<HTMLCanvasElement | null>(null);
  let width = $state(0);
  let dragging = false;

  const css = (name: string) => (canvas ? getComputedStyle(canvas).getPropertyValue(name).trim() : "#888");

  $effect(() => {
    if (!canvas || width === 0) return;
    const dpr = window.devicePixelRatio || 1;
    canvas.width = Math.round(width * dpr);
    canvas.height = Math.round(HEIGHT * dpr);
    const g = canvas.getContext("2d");
    if (!g) return;
    g.scale(dpr, dpr);
    g.clearRect(0, 0, width, HEIGHT);
    const mid = HEIGHT / 2;
    const half = mid - 2;

    if (peaks && peaks.length > 0) {
      const lo = css("--w-lo");
      const md = css("--w-mid");
      const hi = css("--w-hi");
      const step = peaks.length / width;
      for (let x = 0; x < width; x++) {
        const from = Math.floor(x * step);
        const to = Math.max(from + 1, Math.floor((x + 1) * step));
        // the loudest column that falls in this pixel, so narrow peaks survive downscaling
        let a = 0;
        let b = 0;
        let c = 0;
        for (let i = from; i < to && i < peaks.length; i++) {
          a = Math.max(a, peaks[i][0]);
          b = Math.max(b, peaks[i][1]);
          c = Math.max(c, peaks[i][2]);
        }
        const hl = Math.max(1, (a / 255) * half);
        const hm = Math.max(1, (b / 255) * half * 0.75);
        const hh = Math.max(1, (c / 255) * half * 0.45);
        g.fillStyle = lo;
        g.fillRect(x, mid - hl, 1, hl * 2);
        g.fillStyle = md;
        g.fillRect(x, mid - hm, 1, hm * 2);
        g.fillStyle = hi;
        g.fillRect(x, mid - hh, 1, hh * 2);
      }
    } else {
      g.fillStyle = css("--line-hi");
      g.fillRect(0, mid, width, 1);
    }

    if (duration > 0) {
      const px = (s: number) => (s / duration) * width;
      if (loop) {
        g.fillStyle = css("--accent-soft");
        g.fillRect(px(loop.start), 0, Math.max(1, px(loop.end) - px(loop.start)), HEIGHT);
        g.fillStyle = css("--accent");
        g.fillRect(Math.round(px(loop.start)), 0, 1, HEIGHT);
        g.fillRect(Math.round(px(loop.end)), 0, 1, HEIGHT);
      }
      if (beats.length > 1) {
        // below ~3 px per beat the ticks turn to a smear: draw the bar starts only
        const dense = (px(beats[beats.length - 1]) - px(beats[0])) / (beats.length - 1) < 3;
        const first = firstDownbeat ?? 0;
        g.globalAlpha = 0.8;
        for (let i = 0; i < beats.length; i++) {
          const bar = (((i - first) % 4) + 4) % 4 === 0;
          if (dense && !bar) continue;
          g.fillStyle = bar ? css("--text") : css("--line-hi");
          g.fillRect(Math.round(px(beats[i])), 0, bar ? 2 : 1, HEIGHT);
        }
        g.globalAlpha = 1;
      }
      if (cue !== null) {
        const x = Math.round(px(cue));
        g.fillStyle = css("--w-cue");
        g.fillRect(x, 0, 2, HEIGHT);
        g.beginPath();
        g.moveTo(x, 0);
        g.lineTo(x + 10, 0);
        g.lineTo(x, 10);
        g.fill();
      }
    }
  });

  function fraction(e: PointerEvent) {
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    return Math.min(1, Math.max(0, (e.clientX - r.left) / r.width));
  }
  function down(e: PointerEvent) {
    if (disabled || e.button !== 0) return;
    dragging = true;
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    onseek?.(fraction(e));
  }
  function move(e: PointerEvent) {
    if (dragging) onseek?.(fraction(e));
  }
  function up() {
    dragging = false;
  }
</script>

<div
  class="wave"
  class:disabled
  bind:clientWidth={width}
  role="slider"
  tabindex="-1"
  aria-label="Track position"
  aria-valuemin="0"
  aria-valuemax="100"
  aria-valuenow={Math.round(progress * 100)}
  onpointerdown={down}
  onpointermove={move}
  onpointerup={up}
  onpointercancel={up}
>
  <canvas bind:this={canvas} style:width="{width}px" style:height="{HEIGHT}px"></canvas>
  {#if !disabled}<div class="head" style:left="{Math.min(1, Math.max(0, progress)) * 100}%"></div>{/if}
  {#if busy}<div class="note">loading…</div>{/if}
</div>

<style>
  .wave {
    /* the mockup's waveform colours: low pink, mid lilac, high green, cue yellow */
    --w-lo: #ff48a8;
    --w-mid: #e070ff;
    --w-hi: var(--green);
    --w-cue: #ffd23b;
    position: relative;
    height: 48px;
    background: var(--deep);
    border: 1px solid var(--line);
    border-radius: 4px;
    overflow: hidden;
    cursor: pointer;
    touch-action: none;
    user-select: none;
  }
  .wave.disabled {
    cursor: default;
    opacity: 0.6;
  }
  canvas {
    display: block;
  }
  .head {
    position: absolute;
    top: 0;
    bottom: 0;
    width: 2px;
    margin-left: -1px;
    background: var(--green);
    box-shadow: 0 0 6px var(--green);
    pointer-events: none;
  }
  .note {
    position: absolute;
    right: 8px;
    top: 4px;
    font-size: 11px;
    color: var(--muted);
    pointer-events: none;
  }
</style>
