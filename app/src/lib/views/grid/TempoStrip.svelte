<script lang="ts">
  import { driftClass, gapBpms } from "./vtempoLogic";
  import { lowerBound } from "./gridMath";
  import { appearance } from "$lib/state/appearance.svelte";
  import { bitmapSize, fitBitmap } from "./layers";

  // Two strips under the waveform, on the same time axis (same viewStart / pxPerSec): the tempo
  // the beat positions imply (a step line, one step per beat, with the sections labelled) and,
  // once the grid has been checked against the audio, how far each beat line is from its onset
  // (a dot per beat, a band for the drift threshold, the drifting spans shaded). Drawing only.
  let {
    beatSecs,
    sections,
    offsets,
    biasMs,
    thresholdMs,
    spans,
    duration,
    playhead,
    viewStart,
    pxPerSec,
    width,
    height = 118,
    checking = false,
    onseek,
  }: {
    beatSecs: ArrayLike<number>;
    /** Sections as seconds: start, end, tempo, and whether the user cut there. */
    sections: { start: number; end: number; bpm: number; forced: boolean }[];
    /** Per beat: onset minus beat line in ms, null where none; null when not checked yet. */
    offsets: (number | null)[] | null;
    biasMs: number;
    thresholdMs: number;
    /** Drifting spans in seconds. */
    spans: { start: number; end: number }[];
    duration: number;
    playhead: number;
    viewStart: number;
    pxPerSec: number;
    width: number;
    height?: number;
    checking?: boolean;
    onseek?: (secs: number) => void;
  } = $props();

  let canvas = $state<HTMLCanvasElement | null>(null);
  let host = $state<HTMLDivElement | null>(null);

  const css = (name: string) => (host ? getComputedStyle(host).getPropertyValue(name).trim() || "#888" : "#888");
  const tempoH = $derived(Math.round(height * 0.42));
  const bpms = $derived(gapBpms(beatSecs));

  // vertical range of the tempo curve: the grid's own range, padded, never narrower than 4 BPM
  const range = $derived.by(() => {
    let lo = Infinity;
    let hi = -Infinity;
    for (const b of bpms) {
      if (b <= 0) continue;
      if (b < lo) lo = b;
      if (b > hi) hi = b;
    }
    if (!Number.isFinite(lo)) return [0, 1] as const;
    const pad = Math.max(2 - (hi - lo) / 2, (hi - lo) * 0.12);
    return [lo - pad, hi + pad] as const;
  });

  // Everything but the playhead goes into an off-screen layer, redrawn only when the view, the
  // grid, the drift or the size change; each frame of playback blits it and draws the playhead.
  let layer: HTMLCanvasElement | null = null;
  let layerVersion = $state(0);
  let layerCount = 0;
  let L = { t0: 0, pps: 1, W: 0, H: 0, dpr: 1, text: "#888" };
  let paintedX = Number.NaN;

  $effect(() => {
    const c = canvas;
    const W = width;
    const pps = pxPerSec;
    void appearance.waveBands;
    if (!c || W === 0 || pps === 0) return;
    const H = height;
    const dpr = window.devicePixelRatio || 1;
    layer ??= document.createElement("canvas");
    const [bw, bh] = bitmapSize(W, H, dpr);
    fitBitmap(layer, bw, bh);
    const g = layer.getContext("2d");
    if (!g) return;
    g.setTransform(dpr, 0, 0, dpr, 0, 0);
    g.clearRect(0, 0, W, H);
    const t0 = viewStart;
    const t1 = t0 + W / pps;
    const X = (t: number) => (t - t0) * pps;
    g.fillStyle = css("--deep");
    g.fillRect(0, 0, W, H);
    g.font = "11px system-ui, sans-serif";
    g.textBaseline = "top";

    // ---- tempo curve ----
    const [lo, hi] = range;
    const Y = (b: number) => 14 + (1 - (b - lo) / (hi - lo)) * (tempoH - 20);
    g.fillStyle = css("--muted");
    g.fillText("tempo", 4, 1);
    if (beatSecs.length > 1) {
      g.strokeStyle = css("--text");
      g.lineWidth = 1.5;
      g.beginPath();
      let started = false;
      let i = Math.max(0, lowerBound(beatSecs, t0) - 1);
      for (; i < beatSecs.length && beatSecs[i] <= t1 + 1; i++) {
        const next = i + 1 < beatSecs.length ? beatSecs[i + 1] : beatSecs[i] + (beatSecs[i] - beatSecs[i - 1]);
        const y = Y(bpms[i]);
        if (!started) {
          g.moveTo(X(beatSecs[i]), y);
          started = true;
        } else g.lineTo(X(beatSecs[i]), y);
        g.lineTo(X(next), y);
      }
      g.stroke();
    }
    // section edges and tempo labels
    for (const s of sections) {
      if (s.end < t0 || s.start > t1) continue;
      const x = Math.round(X(s.start));
      g.strokeStyle = s.forced ? css("--cue") : css("--line-hi");
      g.lineWidth = 1;
      g.setLineDash([3, 3]);
      g.beginPath();
      g.moveTo(x + 0.5, 0);
      g.lineTo(x + 0.5, H);
      g.stroke();
      g.setLineDash([]);
      const room = (Math.min(s.end, t1) - Math.max(s.start, t0)) * pps;
      if (room > 58) {
        g.fillStyle = css("--text");
        g.fillText(s.bpm.toFixed(2), Math.max(x, 0) + 4, tempoH - 14);
      }
    }

    // ---- drift ----
    const dTop = tempoH + 4;
    const dMid = dTop + (H - dTop) / 2;
    const dHalf = (H - dTop) / 2 - 3;
    const reach = Math.max(60, thresholdMs * 2.5); // ms at the edge of the strip
    const DY = (ms: number) => dMid - (Math.max(-reach, Math.min(reach, ms - biasMs)) / reach) * dHalf;
    g.fillStyle = css("--line");
    g.fillRect(0, tempoH + 1, W, 1);
    g.fillStyle = css("--muted");
    g.fillText(offsets ? `drift (ms from the median ${biasMs.toFixed(1)})` : "drift", 4, dTop - 1);
    if (!offsets) {
      g.textAlign = "center";
      g.fillText(checking ? "Reading the audio..." : "Check the grid against the audio to see drift", W / 2, dMid - 6);
      g.textAlign = "left";
    } else {
      for (const s of spans) {
        if (s.end < t0 || s.start > t1) continue;
        g.fillStyle = css("--bad-bg");
        g.fillRect(X(s.start), dTop, Math.max(2, (s.end - s.start) * pps), H - dTop);
      }
      g.fillStyle = css("--accent-soft");
      g.fillRect(0, DY(biasMs + thresholdMs), W, DY(biasMs - thresholdMs) - DY(biasMs + thresholdMs));
      g.fillStyle = css("--line-hi");
      g.fillRect(0, Math.round(dMid), W, 1);
      const bad = css("--bad");
      const ok = css("--ok");
      for (let k = lowerBound(beatSecs, t0 - 0.2); k < beatSecs.length && beatSecs[k] <= t1 + 0.2; k++) {
        const o = offsets[k];
        if (o === null || o === undefined) continue;
        const off = driftClass(o, biasMs, thresholdMs) === "off";
        g.globalAlpha = off ? 1 : 0.65;
        g.fillStyle = off ? bad : ok;
        g.beginPath();
        g.arc(X(beatSecs[k]), DY(o), off ? 3 : 2, 0, Math.PI * 2);
        g.fill();
      }
      g.globalAlpha = 1;
    }

    L = { t0, pps, W, H, dpr, text: css("--text") };
    paintedX = Number.NaN;
    layerVersion = ++layerCount;
  });

  // ---- playhead ----
  $effect(() => {
    const c = canvas;
    const head = playhead;
    void layerVersion;
    if (!c || !layer || L.W === 0) return;
    const { t0, pps, W, H, dpr } = L;
    const x = head >= t0 && head <= t0 + W / pps ? Math.round((head - t0) * pps) - 1 : -1e9;
    if (x === paintedX && c.width === layer.width && c.height === layer.height) return;
    paintedX = x;
    fitBitmap(c, layer.width, layer.height);
    const g = c.getContext("2d");
    if (!g) return;
    g.setTransform(1, 0, 0, 1, 0, 0);
    g.clearRect(0, 0, c.width, c.height);
    g.drawImage(layer, 0, 0);
    if (x > -1e8) {
      g.setTransform(dpr, 0, 0, dpr, 0, 0);
      g.fillStyle = L.text;
      g.fillRect(x, 0, 2, H);
    }
  });

  function click(e: MouseEvent) {
    const x = e.clientX - (e.currentTarget as HTMLElement).getBoundingClientRect().left;
    onseek?.(Math.max(0, Math.min(duration, viewStart + x / pxPerSec)));
  }
</script>

<div class="strip" bind:this={host} style:height="{height}px">
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions, a11y_no_interactive_element_to_noninteractive_role -->
  <canvas
    bind:this={canvas}
    style:width="{width}px"
    style:height="{height}px"
    role="img"
    aria-label="Tempo of the beat grid over time, and how far each beat line is from the audio's onsets. Click to place the playhead."
    onclick={click}
  ></canvas>
</div>

<style>
  .strip {
    position: relative;
    width: 100%;
    margin-top: 6px;
    background: var(--deep);
    border: 1px solid var(--line);
    border-radius: 4px;
    overflow: hidden;
  }
  canvas {
    display: block;
    cursor: pointer;
  }
</style>
