<script lang="ts">
  import type { Peak } from "$lib/api/analysis";
  import { bandsOver, clamp, fmtTime, isDownbeat, lowerBound, tickStep } from "./gridMath";

  // Zoomable colour waveform with beat lines. Low band pink, mid lilac, high green; bar lines
  // white and bold, other beats thin green; the device cue is a yellow flag; the playhead white.
  // Wheel zooms around the pointer (Shift or sideways wheel pans), drag pans, a click seeks.
  let {
    peaks,
    beatSecs,
    firstDownbeat,
    duration,
    cue,
    playhead,
    viewStart = $bindable(0),
    pxPerSec = $bindable(0),
    width = $bindable(0),
    height = 280,
    onseek,
  }: {
    peaks: Peak[] | null;
    beatSecs: ArrayLike<number>;
    firstDownbeat: number | null;
    duration: number;
    /** Device cue in seconds, or null. */
    cue: number | null;
    playhead: number;
    viewStart?: number;
    pxPerSec?: number;
    width?: number;
    height?: number;
    onseek?: (secs: number) => void;
  } = $props();

  const MAX_PX_PER_SEC = 4000;
  const RULER = 18;

  let canvas = $state<HTMLCanvasElement | null>(null);
  let host = $state<HTMLDivElement | null>(null);

  // while the whole track is showing, a resize keeps showing all of it
  let showingAll = true;
  const fitPx = () => (duration > 0 && width > 0 ? width / duration : 1);

  function clampView(start: number, pps: number) {
    const span = width / pps;
    return clamp(start, 0, Math.max(0, duration - span));
  }

  /** Zooms by `factor` keeping the time under `anchorPx` (default: the centre) fixed. */
  export function zoom(factor: number, anchorPx = width / 2) {
    if (width === 0 || duration <= 0) return;
    const anchorT = viewStart + anchorPx / pxPerSec;
    const next = clamp(pxPerSec * factor, fitPx(), MAX_PX_PER_SEC);
    pxPerSec = next;
    showingAll = next <= fitPx() * 1.001;
    viewStart = clampView(anchorT - anchorPx / next, next);
  }

  export function fit() {
    showingAll = true;
    pxPerSec = fitPx();
    viewStart = 0;
  }

  /** Centres the view on `secs`. */
  export function centre(secs: number) {
    viewStart = clampView(secs - width / pxPerSec / 2, pxPerSec);
  }

  // first layout, and a new track: show the whole track
  let fittedFor = -1;
  $effect(() => {
    if (width > 0 && duration > 0 && (fittedFor !== duration || pxPerSec === 0)) {
      fittedFor = duration;
      fit();
    }
  });

  // keep the zoom floor when the window is resized
  $effect(() => {
    if (width > 0 && pxPerSec > 0 && (showingAll || pxPerSec < fitPx())) fit();
  });

  const css = (name: string) => (host ? getComputedStyle(host).getPropertyValue(name).trim() || "#888" : "#888");

  $effect(() => {
    const c = canvas;
    const W = width;
    const pps = pxPerSec;
    if (!c || W === 0 || pps === 0) return;
    const H = height;
    const dpr = window.devicePixelRatio || 1;
    c.width = Math.round(W * dpr);
    c.height = Math.round(H * dpr);
    const g = c.getContext("2d");
    if (!g) return;
    g.scale(dpr, dpr);
    const t0 = viewStart;
    const t1 = t0 + W / pps;
    const top = RULER;
    const mid = top + (H - top) / 2;
    const half = (H - top) / 2 - 2;

    g.fillStyle = css("--deep");
    g.fillRect(0, 0, W, H);

    // waveform
    if (peaks && peaks.length > 0 && duration > 0) {
      const cols = [css("--g-low"), css("--g-mid"), css("--g-high")];
      const scale = [1, 0.75, 0.45];
      for (let x = 0; x < W; x++) {
        const f0 = (t0 + x / pps) / duration;
        const f1 = (t0 + (x + 1) / pps) / duration;
        if (f1 <= 0 || f0 >= 1) continue;
        const b = bandsOver(peaks, Math.max(0, f0), Math.min(1, f1));
        for (let k = 0; k < 3; k++) {
          const h = (b[k] / 255) * half * scale[k];
          if (h <= 0) continue;
          g.fillStyle = cols[k];
          g.fillRect(x, mid - h, 1, h * 2);
        }
      }
    } else {
      g.fillStyle = css("--line");
      g.fillRect(0, mid, W, 1);
    }

    // ruler
    const step = tickStep(pps, 90);
    g.font = "11px system-ui, sans-serif";
    g.textBaseline = "top";
    g.fillStyle = css("--muted");
    for (let t = Math.ceil(t0 / step) * step; t <= t1; t += step) {
      const x = Math.round((t - t0) * pps);
      g.fillRect(x, RULER - 5, 1, 5);
      g.fillText(step < 1 ? fmtTime(t) : fmtTime(t).slice(0, -4), x + 3, 2);
    }

    // beat lines
    const line = css("--g-beat");
    const bar = css("--text");
    let i = lowerBound(beatSecs, t0 - 0.01);
    const gapPx = beatSecs.length > i + 1 ? (beatSecs[i + 1] - beatSecs[i]) * pps : 0;
    const showBeats = gapPx >= 4;
    const showBars = gapPx * 4 >= 3;
    g.textBaseline = "top";
    for (; i < beatSecs.length && beatSecs[i] <= t1 + 0.01; i++) {
      const down = isDownbeat(i, firstDownbeat);
      if (down ? !showBars : !showBeats) continue;
      const x = Math.round((beatSecs[i] - t0) * pps);
      g.globalAlpha = down ? 0.95 : 0.7;
      g.fillStyle = down ? bar : line;
      g.fillRect(x - (down ? 1 : 0), top, down ? 2 : 1, H - top);
      if (down && gapPx * 4 > 70) {
        g.globalAlpha = 1;
        g.fillStyle = bar;
        g.fillText(String((i - (firstDownbeat ?? 0)) / 4 + 1), x + 4, top + 2);
      }
    }
    g.globalAlpha = 1;

    // device cue
    if (cue !== null && cue >= t0 - 0.05 && cue <= t1) {
      const x = Math.round((cue - t0) * pps);
      g.fillStyle = css("--g-cue");
      g.fillRect(x, top, 2, H - top);
      g.beginPath();
      g.moveTo(x, top);
      g.lineTo(x + 12, top);
      g.lineTo(x, top + 12);
      g.fill();
    }

    // playhead
    if (playhead >= t0 && playhead <= t1) {
      g.fillStyle = css("--text");
      g.fillRect(Math.round((playhead - t0) * pps) - 1, 0, 2, H);
    }
  });

  // wheel needs preventDefault, so it is attached non-passive by hand
  $effect(() => {
    const c = canvas;
    if (!c) return;
    const onwheel = (e: WheelEvent) => {
      e.preventDefault();
      if (e.shiftKey || Math.abs(e.deltaX) > Math.abs(e.deltaY)) {
        const d = (e.shiftKey && e.deltaX === 0 ? e.deltaY : e.deltaX) / pxPerSec;
        viewStart = clampView(viewStart + d, pxPerSec);
      } else {
        zoom(Math.exp(-e.deltaY * 0.0015), e.clientX - c.getBoundingClientRect().left);
      }
    };
    c.addEventListener("wheel", onwheel, { passive: false });
    return () => c.removeEventListener("wheel", onwheel);
  });

  let drag: { x: number; start: number; moved: boolean } | null = null;

  function down(e: PointerEvent) {
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    drag = { x: e.clientX, start: viewStart, moved: false };
  }

  function move(e: PointerEvent) {
    if (!drag) return;
    const dx = e.clientX - drag.x;
    if (Math.abs(dx) > 3) drag.moved = true;
    if (drag.moved) viewStart = clampView(drag.start - dx / pxPerSec, pxPerSec);
  }

  function up(e: PointerEvent) {
    const d = drag;
    drag = null;
    if (d && !d.moved) {
      const x = e.clientX - (e.currentTarget as HTMLElement).getBoundingClientRect().left;
      onseek?.(clamp(viewStart + x / pxPerSec, 0, duration));
    }
  }
</script>

<div class="wave" bind:this={host} bind:clientWidth={width} style:height="{height}px">
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions, a11y_no_noninteractive_tabindex -->
  <canvas
    bind:this={canvas}
    style:width="{width}px"
    style:height="{height}px"
    aria-label="Waveform with beat grid. Click to place the playhead, drag to pan, scroll to zoom."
    onpointerdown={down}
    onpointermove={move}
    onpointerup={up}
    onpointercancel={() => (drag = null)}
  ></canvas>
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
    cursor: crosshair;
    touch-action: none;
  }
</style>
