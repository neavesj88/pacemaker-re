<script lang="ts">
  // The colour waveform and beat ticks of one clip, drawn on a canvas that covers only the part
  // of the clip near the viewport (so a 10-minute clip at high zoom is still a small canvas).
  // `from`..`to` are pixels from the clip's audible start; the parent keeps them within the clip.
  import type { Clip } from "$lib/api/mixes";
  import { untrack } from "svelte";
  import { TempoMap, clipBeatPoints } from "$lib/state/mixedit";
  import { BAND_COLOURS, SR } from "./layout";
  import { peakCache } from "./peaks.svelte";

  let { clip, pps, from, to, height }: { clip: Clip; pps: number; from: number; to: number; height: number } = $props();

  let canvas = $state<HTMLCanvasElement | null>(null);
  const width = $derived(Math.max(1, Math.ceil(to - from)));

  const peaks = $derived(clip.track ? peakCache.get(clip.track.location, clip.track.durationSecs) : null);

  // Moving a clip changes only `start`, which does not change this picture (everything is drawn
  // relative to the audible start), so the effect depends on the parts that do, not on `clip`.
  $effect(() => {
    const cv = canvas;
    if (!cv) return;
    const key = [cv, clip.tempo, clip.trimStart, clip.trimEnd, clip.length, clip.track?.grid, pps, from, to, height, peaks];
    if (last && key.every((v, i) => v === last![i])) return;
    last = key;
    untrack(() => draw(cv));
  });
  let last: unknown[] | null = null;

  function draw(cv: HTMLCanvasElement) {
    const dpr = window.devicePixelRatio || 1;
    const w = width;
    cv.width = Math.round(w * dpr);
    cv.height = Math.round(height * dpr);
    const g = cv.getContext("2d");
    if (!g) return;
    g.setTransform(dpr, 0, 0, dpr, 0, 0);
    g.clearRect(0, 0, w, height);

    const tm = new TempoMap(clip.tempo);
    const total = tm.sourceAt(clip.length) || 1;
    // clip-local sample at pixel x (x counted from the clip's audible start)
    const localAt = (x: number) => clip.trimStart + ((from + x) / pps) * SR;
    const mid = height / 2;

    if (peaks && peaks.length > 0) {
      const n = peaks.length;
      let prev = Math.floor((tm.sourceAt(localAt(0)) / total) * n);
      for (let x = 0; x < w; x++) {
        const next = Math.floor((tm.sourceAt(localAt(x + 1)) / total) * n);
        let lo = 0;
        let md = 0;
        let hi = 0;
        // the strongest column this pixel covers, so thin peaks survive zooming out
        const a = Math.min(n - 1, Math.max(0, prev));
        const b = Math.min(n - 1, Math.max(a, next - 1));
        for (let i = a; i <= b; i++) {
          const p = peaks[i];
          if (p[0] > lo) lo = p[0];
          if (p[1] > md) md = p[1];
          if (p[2] > hi) hi = p[2];
        }
        prev = next;
        const amp = height - 4;
        g.fillStyle = BAND_COLOURS.low;
        let h = Math.max(1, (lo / 255) * amp);
        g.fillRect(x, mid - h / 2, 1, h);
        g.fillStyle = BAND_COLOURS.mid;
        h = Math.max(0, (md / 255) * amp);
        g.fillRect(x, mid - h / 2, 1, h);
        g.fillStyle = BAND_COLOURS.high;
        h = Math.max(0, (hi / 255) * amp);
        g.fillRect(x, mid - h / 2, 1, h);
      }
    } else {
      g.fillStyle = "rgba(95,154,111,0.35)";
      g.fillRect(0, mid - 1, w, 2);
    }

    // beat ticks: every beat a short tick at the bottom, bars longer, phrases a full faint line
    for (const b of clipBeatPoints({ ...clip, start: 0 })) {
      const x = ((b.time - clip.trimStart) / SR) * pps - from;
      if (x < -1 || x > w + 1) continue;
      if (b.kind === "beat" && pps < 14) continue;
      if (b.kind === "phrase") {
        g.fillStyle = "rgba(255,255,255,0.28)";
        g.fillRect(Math.round(x), 0, 1, height);
      } else if (b.kind === "bar") {
        g.fillStyle = "rgba(255,255,255,0.55)";
        g.fillRect(Math.round(x), height - 11, 1, 11);
      } else {
        g.fillStyle = "rgba(255,255,255,0.4)";
        g.fillRect(Math.round(x), height - 5, 1, 5);
      }
    }
  }
</script>

<canvas bind:this={canvas} style:left="{from}px" style:width="{width}px" style:height="{height}px" aria-hidden="true"></canvas>

<style>
  canvas {
    position: absolute;
    top: 0;
    pointer-events: none;
  }
</style>
