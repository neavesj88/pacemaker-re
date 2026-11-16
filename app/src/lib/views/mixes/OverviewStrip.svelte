<script lang="ts">
  // The whole mix in one thin strip above the ruler: every clip in its colour (overlaps show
  // through each other), the playhead, and a box over the part the timeline below shows. Click in
  // the strip to centre the view there, drag the box to scroll; the box gets narrower as you zoom in.
  //
  // The picture (clips and their real waveform peaks, one column of peaks per pixel, the same
  // drawing as the clips on the timeline, just smaller) is drawn once on the canvas and redrawn only
  // when the mix, the peaks, the colours or the size change. While a mix plays only the box and the
  // playhead move: they are two elements, not part of the picture.
  import { untrack } from "svelte";
  import { TempoMap } from "$lib/state/mixedit";
  import { mixes } from "$lib/state/mixes.svelte";
  import { HEADER_W, SR, clipBands, clipColour, maxScroll } from "./layout";
  import { BOX_MIN_W, centreScroll, dragScroll, onBox, overviewSpan, stripX, visibleBox } from "./overview";
  import { peakCache } from "./peaks.svelte";
  import { mv } from "./view.svelte";

  /** `inset`: the width of the timeline's vertical scrollbar, so the strip ends where the lanes end. */
  let { height, inset = 0 }: { height: number; inset?: number } = $props();

  const doc = $derived(mixes.doc!);
  let area = $state<HTMLDivElement | null>(null);
  let canvas = $state<HTMLCanvasElement | null>(null);
  let w = $state(0);
  let h = $state(0);
  /** The screen's pixel ratio. It changes when the window moves to another screen (the 5120 monitor, a laptop), with no change of size in CSS pixels. */
  let dpr = $state(window.devicePixelRatio || 1);
  $effect(() => {
    const mq = matchMedia(`(resolution: ${dpr}dppx)`);
    const changed = () => (dpr = window.devicePixelRatio || 1);
    mq.addEventListener("change", changed, { once: true });
    return () => mq.removeEventListener("change", changed);
  });
  /** The pointer is over the box: the cursor says it can be dragged. */
  let overBox = $state(false);

  const span = $derived(overviewSpan(doc.length / SR));
  const laneW = $derived(Math.max(0, mv.viewW - HEADER_W));
  const box = $derived(visibleBox(mv.scrollLeft, laneW, mv.pps, span, w));
  const headX = $derived(stripX(mv.playhead / SR, span, w));

  $effect(() => {
    const el = area;
    if (!el) return;
    const size = () => {
      w = el.clientWidth;
      h = el.clientHeight;
    };
    const ro = new ResizeObserver(size);
    ro.observe(el);
    size();
    return () => ro.disconnect();
  });

  // ---- the picture ----------------------------------------------------------------------------

  let last: unknown[] | null = null;
  $effect(() => {
    const cv = canvas;
    if (!cv || w < 2 || h < 2) return;
    const clips = doc.clips;
    // everything that changes the picture; the playhead and the box are not in it
    const key: unknown[] = [cv, w, h, dpr, span];
    for (const c of clips) key.push(c.id, c.visibleStart, c.visibleEnd, c.trimStart, c.length, c.tempo, c.clipBypassed, peakCache.get(c.track?.location ?? "", c.track?.durationSecs ?? 0));
    if (last && last.length === key.length && key.every((v, i) => v === last![i])) return;
    last = key;
    untrack(() => draw(cv, dpr));
  });

  function draw(cv: HTMLCanvasElement, dpr: number) {
    cv.width = Math.round(w * dpr);
    cv.height = Math.round(h * dpr);
    const g = cv.getContext("2d");
    if (!g) return;
    g.setTransform(dpr, 0, 0, dpr, 0, 0);
    g.clearRect(0, 0, w, h);
    const perSample = w / (span * SR);
    const mid = h / 2;
    const amp = h - 4;
    doc.clips.forEach((clip, idx) => {
      const colour = clipColour(idx);
      const bands = clipBands(colour);
      const dim = clip.clipBypassed ? 0.5 : 1;
      const x0 = clip.visibleStart * perSample;
      const x1 = Math.max(x0 + 1, clip.visibleEnd * perSample);
      g.globalAlpha = dim;
      g.fillStyle = colour + "29";
      g.fillRect(x0, 0, x1 - x0, h);
      const peaks = clip.track ? peakCache.get(clip.track.location, clip.track.durationSecs) : null;
      if (peaks && peaks.length > 0) {
        const n = peaks.length;
        const tm = new TempoMap(clip.tempo);
        const total = tm.sourceAt(clip.length) || 1;
        // clip-local sample at strip pixel x
        const localAt = (x: number) => clip.trimStart + Math.max(0, x / perSample - clip.visibleStart);
        const from = Math.floor(x0);
        const to = Math.ceil(x1);
        let prev = Math.floor((tm.sourceAt(localAt(from)) / total) * n);
        g.globalAlpha = dim * 0.85;
        for (let x = from; x < to; x++) {
          const next = Math.floor((tm.sourceAt(localAt(x + 1)) / total) * n);
          let lo = 0;
          let md = 0;
          let hi = 0;
          // the strongest column this pixel covers, as on the timeline
          const a = Math.min(n - 1, Math.max(0, prev));
          const b = Math.min(n - 1, Math.max(a, next - 1));
          for (let i = a; i <= b; i++) {
            const p = peaks[i];
            if (p[0] > lo) lo = p[0];
            if (p[1] > md) md = p[1];
            if (p[2] > hi) hi = p[2];
          }
          prev = next;
          g.fillStyle = bands.low;
          let bh = Math.max(1, (lo / 255) * amp);
          g.fillRect(x, mid - bh / 2, 1, bh);
          g.fillStyle = bands.mid;
          bh = (md / 255) * amp;
          g.fillRect(x, mid - bh / 2, 1, bh);
          g.fillStyle = bands.high;
          bh = (hi / 255) * amp;
          g.fillRect(x, mid - bh / 2, 1, bh);
        }
      }
      g.globalAlpha = dim * 0.9;
      g.strokeStyle = colour;
      g.lineWidth = 1;
      g.strokeRect(x0 + 0.5, 0.5, Math.max(0, x1 - x0 - 1), h - 1);
    });
    g.globalAlpha = 1;
  }

  // ---- pointer ----------------------------------------------------------------------------------

  const maxPx = () => maxScroll(mv.endSamples, mv.pps, mv.viewW);

  /** A press on the box drags it; a press elsewhere first centres the view there, then drags too. A
   * move of the view by hand pauses following, like scrolling the timeline does. */
  function down(e: PointerEvent) {
    const el = area;
    if (e.button !== 0 || !el || w <= 0) return;
    e.preventDefault();
    el.setPointerCapture(e.pointerId);
    el.focus({ preventScroll: true });
    const rect = el.getBoundingClientRect();
    const at = (ev: PointerEvent) => ev.clientX - rect.left;
    const x0 = at(e);
    mv.pauseFollow();
    if (!onBox(x0, box)) mv.scrollTo(centreScroll(x0, w, span, mv.pps, laneW, maxPx()));
    const startScroll = mv.scrollLeft;
    const move = (ev: PointerEvent) => {
      mv.pauseFollow();
      mv.scrollTo(dragScroll(startScroll, at(ev) - x0, w, span, mv.pps, maxPx()));
    };
    const up = () => {
      el.removeEventListener("pointermove", move);
      el.removeEventListener("pointerup", up);
      el.removeEventListener("pointercancel", up);
    };
    el.addEventListener("pointermove", move);
    el.addEventListener("pointerup", up);
    el.addEventListener("pointercancel", up);
  }

  function hover(e: PointerEvent) {
    const el = area;
    overBox = !!el && onBox(e.clientX - el.getBoundingClientRect().left, box);
  }

  function key(e: KeyboardEvent) {
    if (e.key !== "ArrowLeft" && e.key !== "ArrowRight" && e.key !== "Home" && e.key !== "End") return;
    e.preventDefault();
    e.stopPropagation();
    mv.pauseFollow();
    if (e.key === "Home") mv.scrollTo(0);
    else if (e.key === "End") mv.scrollTo(maxPx());
    else mv.scrollTo(mv.scrollLeft + (e.key === "ArrowRight" ? 1 : -1) * Math.max(40, laneW * 0.25));
  }
</script>

<div class="ov" style:height="{height}px" style:padding-right="{inset}px">
  <div class="lab">
    <b>Whole mix</b>
    <span class="muted">click or drag</span>
  </div>
  <div
    class="area"
    bind:this={area}
    role="slider"
    tabindex="0"
    aria-label="Whole mix: the box is the part shown below. Click to centre the view, drag the box, or use the arrow keys."
    aria-valuemin="0"
    aria-valuemax={Math.round(maxPx())}
    aria-valuenow={Math.round(mv.scrollLeft)}
    title="The whole mix. The box is the part shown below: drag it, or click to centre the view there."
    class:grab={overBox}
    onpointerdown={down}
    onpointermove={hover}
    onpointerleave={() => (overBox = false)}
    onkeydown={key}
  >
    <canvas bind:this={canvas} style:width="{w}px" style:height="{h}px" aria-hidden="true"></canvas>
    {#if w > 0}
      <div class="box" style:left="{box.x0}px" style:width="{Math.max(BOX_MIN_W, box.x1 - box.x0)}px" aria-hidden="true"></div>
      <div class="head" style:left="{headX}px" aria-hidden="true"></div>
    {/if}
  </div>
</div>

<style>
  .ov {
    display: flex;
    box-sizing: border-box;
    border-bottom: 1px solid var(--line);
    background: var(--deep);
    user-select: none;
  }
  .lab {
    flex: 0 0 132px;
    width: 132px;
    box-sizing: border-box;
    padding: 4px 8px;
    background: var(--panel);
    border-right: 1px solid var(--line);
    font-size: 12px;
    display: flex;
    flex-direction: column;
    justify-content: center;
    gap: 1px;
    overflow: hidden;
  }
  .lab b {
    color: var(--text);
    font-weight: 600;
  }
  .muted {
    color: var(--muted);
    font-size: 11px;
    white-space: nowrap;
  }
  .area {
    position: relative;
    flex: 1;
    min-width: 0;
    cursor: pointer;
    overflow: hidden;
    outline: none;
    touch-action: none;
  }
  .area:focus-visible {
    box-shadow: inset 0 0 0 2px var(--text);
  }
  canvas {
    position: absolute;
    left: 0;
    top: 0;
    display: block;
    pointer-events: none;
  }
  .box {
    position: absolute;
    top: 1px;
    bottom: 1px;
    box-sizing: border-box;
    border: 2px solid var(--green);
    background: color-mix(in srgb, var(--green) 14%, transparent);
    /* dims the rest of the strip (the area clips it), so the part shown below stands out */
    box-shadow: 0 0 0 100vmax color-mix(in srgb, var(--deep) 55%, transparent);
    border-radius: 2px;
    pointer-events: none;
  }
  .area.grab {
    cursor: grab;
  }
  .area.grab:active {
    cursor: grabbing;
  }
  .head {
    position: absolute;
    top: 0;
    bottom: 0;
    width: 2px;
    margin-left: -1px;
    background: var(--green);
    box-shadow: 0 0 4px var(--green);
    pointer-events: none;
  }
</style>
