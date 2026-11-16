<script lang="ts">
  // One clip on the single mix bar, in its own colour. Where another clip overlaps it, both are
  // drawn over each other the full height of the bar, see-through, as in the original Editor (the
  // layout is `clipSlices` in layout.ts: no lanes). A click in an overlap takes the clip that came
  // in last; the earlier one is reached on its own stretch, by its edge handles or with Tab. Drag
  // to move (snapped and lined up with the neighbour's beat grid), drag an edge to trim; Shift /
  // Ctrl-click adds to the selection. Decks are automatic (state/mixes.svelte.ts).
  import type { Clip } from "$lib/api/mixes";
  import * as E from "$lib/state/mixedit";
  import { effectiveBpm } from "$lib/state/mixedit";
  import { mixes } from "$lib/state/mixes.svelte";
  import { sections } from "$lib/state/sections.svelte";
  import { exceeds, stretchText } from "$lib/state/tempoMatch";
  import { HEADER_W, SR, clipBands, clipSlices, titleTop as placeTitle } from "./layout";
  import ClipSections from "./ClipSections.svelte";
  import ClipWave from "./ClipWave.svelte";
  import { tempoPrefs } from "./tempoPrefs.svelte";
  import { mv } from "./view.svelte";

  let {
    clip,
    colour,
    height,
    oncontext,
  }: {
    clip: Clip;
    /** This clip's colour on the bar (clipColour of its place in the mix). */
    colour: string;
    height: number;
    oncontext: (e: MouseEvent, clip: Clip) => void;
  } = $props();

  const EDGE = 7;
  /** Height of a crossfade's strip (FadeBlock's HEAD). */
  const FADE_HEAD = 16;
  const left = $derived(mv.x(clip.visibleStart));
  const width = $derived(Math.max(2, mv.x(clip.visibleEnd - clip.visibleStart)));
  const selected = $derived(mixes.selected.includes(clip.id));
  const broken = $derived(!clip.track || !clip.track.available || clip.trackId < 0);
  const bands = $derived(clipBands(colour));
  const speed = $derived(clip.bpm > 0 ? effectiveBpm(clip, clip.trimStart) / clip.bpm : 1);
  const stretchBadge = $derived(exceeds(speed, tempoPrefs.value.warnPct) ? `${stretchText((speed - 1) * 100)} stretch` : "");

  /** The audible part cut where another clip starts or ends: every stretch is the full height of
   * the bar, an overlapped one see-through. In pixels from the clip's left edge. */
  const segs = $derived.by(() => {
    const doc = mixes.doc;
    if (!doc) return [];
    const list = clipSlices(doc.clips, clip.id);
    return list.map((sl, i) => {
      const x0 = mv.x(sl.from - clip.visibleStart);
      const x1 = mv.x(sl.to - clip.visibleStart);
      return { ...sl, x0, x1: Math.max(x0 + 1, x1), first: i === 0, last: i === list.length - 1 };
    });
  });
  /** The name sits at the top of the clip's start, below the crossfade strip when the clip starts
   * inside an overlap (the strip lies over that corner). */
  const titleTop = $derived(placeTitle(segs, FADE_HEAD));
  /** Edge handles sit above the clips lying over them, so an edge buried under the later clip can
   * still be grabbed. */
  const handles = $derived(width > EDGE * 3);
  let box = $state<HTMLDivElement | null>(null);

  // The canvas covers the viewport plus half a screen each side, in 256 px steps, so scrolling
  // redraws rarely.
  const STEP = 256;
  const win = $derived.by(() => {
    const vl = mv.scrollLeft - mv.viewW / 2 - left;
    const vr = mv.scrollLeft + mv.viewW - HEADER_W + mv.viewW / 2 - left;
    const from = Math.max(0, Math.floor(vl / STEP) * STEP);
    const to = Math.min(width, Math.ceil(vr / STEP) * STEP);
    return { from, to: Math.max(from + 1, to) };
  });

  const title = $derived(clip.track?.title || clip.fuzzyId.title || "Unknown track");
  const tempoText = $derived.by(() => {
    if (!clip.bpm) return "";
    const eff = effectiveBpm(clip, clip.trimStart);
    return Math.abs(eff - clip.bpm) < 0.05 ? `${clip.bpm.toFixed(0)} BPM` : `${clip.bpm.toFixed(0)}→${eff.toFixed(0)} BPM`;
  });

  let hover = $state<"left" | "right" | "move">("move");
  /** Which part of the whole clip (not of the slice under the pointer) a pointer is on. */
  function zone(e: PointerEvent | MouseEvent): "left" | "right" | "move" {
    const r = (box ?? (e.currentTarget as HTMLElement)).getBoundingClientRect();
    const x = e.clientX - r.left;
    if (r.width > EDGE * 3) {
      if (x < EDGE) return "left";
      if (x > r.width - EDGE) return "right";
    }
    return "move";
  }

  function down(e: PointerEvent, forced?: "left" | "right") {
    if (e.button !== 0) return;
    e.stopPropagation();
    const z = forced ?? zone(e);
    const additive = e.shiftKey || e.ctrlKey || e.metaKey;
    if (additive) {
      mixes.toggleSelected(clip.id);
      return;
    }
    const wasSelected = selected;
    if (!wasSelected) mixes.select([clip.id]);
    const doc0 = mixes.doc;
    if (!doc0) return;
    const ids = mixes.selected.filter((id) => doc0.clips.some((c) => c.id === id));
    const startX = e.clientX;
    const startY = e.clientY;
    let moved = false;

    if (z === "move") {
      const orig = new Map(ids.map((id) => [id, doc0.clips.find((c) => c.id === id)!.start]));
      const mv_ = (ev: PointerEvent) => {
        if (!moved && Math.abs(ev.clientX - startX) < 3 && Math.abs(ev.clientY - startY) < 3) return;
        moved = true;
        const delta = mv.samples(ev.clientX - startX);
        const snapOn = !ev.altKey && mixes.snap !== "off";
        const primary = orig.get(clip.id)! + delta;
        const info = snapOn ? E.snapClipStartInfo(doc0, clip.id, primary, { mode: mixes.snap, threshold: mixes.threshold, exclude: ids, sections: mixes.sectionLookup }) : null;
        const snapped = info ? info.start : primary;
        mixes.showAlignCue(info?.aligned?.at ?? null);
        const d = snapped - orig.get(clip.id)!;
        mixes.edit(
          "Move clips",
          (doc) => ids.reduce((acc, id) => E.moveClip(acc, id, orig.get(id)! + d, { mode: "off", threshold: 0 }), doc),
          { group: `move:${ids.join(",")}` },
        );
      };
      track(mv_, () => {
        if (!moved && wasSelected && ids.length > 1) mixes.select([clip.id]);
      });
    } else {
      const edge = z;
      const base = edge === "left" ? clip.visibleStart : clip.visibleEnd;
      const rs = (ev: PointerEvent) => {
        moved = true;
        const pos = base + mv.samples(ev.clientX - startX);
        if (ev.altKey) {
          // no snapping: resize against the unsnapped position
          const keep = mixes.snap;
          mixes.snap = "off";
          mixes.resizeClip(clip.id, edge, pos, { live: true });
          mixes.snap = keep;
        } else mixes.resizeClip(clip.id, edge, pos, { live: true });
      };
      track(rs, () => {});
    }
  }

  function track(onmove: (e: PointerEvent) => void, onend: () => void) {
    const up = () => {
      window.removeEventListener("pointermove", onmove);
      window.removeEventListener("pointerup", up);
      window.removeEventListener("pointercancel", up);
      mixes.endGesture();
      onend();
    };
    window.addEventListener("pointermove", onmove);
    window.addEventListener("pointerup", up);
    window.addEventListener("pointercancel", up);
  }

  function key(e: KeyboardEvent) {
    // Space is left to the window handler (play / pause)
    if (e.key === "Enter") {
      e.preventDefault();
      e.stopPropagation();
      mixes.select([clip.id], e.shiftKey);
    }
  }

  function ctx(e: MouseEvent) {
    e.preventDefault();
    e.stopPropagation();
    if (!mixes.selected.includes(clip.id)) mixes.select([clip.id]);
    oncontext(e, clip);
  }
</script>

<div
  class="clip"
  class:selected
  class:broken
  class:bypass={clip.clipBypassed}
  bind:this={box}
  data-clip={clip.id}
  style:left="{left}px"
  style:width="{width}px"
  style:height="{height}px"
  style:--c={colour}
>
  {#each segs as sg (sg.from)}
    <div
      class="seg"
      class:edge={hover !== "move"}
      class:overlap={sg.overlapped}
      class:first={sg.first}
      class:last={sg.last}
      role="button"
      tabindex={sg.first ? 0 : -1}
      aria-label="Clip {title}"
      aria-pressed={selected}
      style:left="{sg.x0}px"
      style:width="{sg.x1 - sg.x0}px"
      style:z-index={1 + Math.min(sg.layer, 1)}
      onpointerdown={down}
      onpointermove={(e) => (hover = zone(e))}
      onkeydown={key}
      oncontextmenu={ctx}
    ></div>
  {/each}
  {#each segs as sg (sg.from)}
    {@const wf = Math.max(sg.x0, win.from)}
    {@const wt = Math.min(sg.x1, win.to)}
    {#if wt > wf}<ClipWave {clip} pps={mv.pps} from={wf} to={wt} height={height - 2} top={1} {bands} opacity={sg.opacity} layer={1 + Math.min(sg.layer, 1)} />{/if}
  {/each}
  {#if handles}
    <!-- the edges stay grabbable where a later clip lies over them -->
    <div class="handle l" role="presentation" style:height="{height}px" onpointerdown={(e) => down(e, "left")} oncontextmenu={ctx}></div>
    <div class="handle r" role="presentation" style:height="{height}px" onpointerdown={(e) => down(e, "right")} oncontextmenu={ctx}></div>
  {/if}
  {#if mv.sectionMarkers && sections.showMarkers}<ClipSections {clip} pps={mv.pps} />{/if}
  <span class="title" style:top="{titleTop}px" style:max-width="{Math.max(0, width - 8)}px">{title}<em>{tempoText}</em></span>
  {#if stretchBadge}
    <span class="stretch" style:top="{titleTop + 16}px" title="This track plays {Math.abs(speed * 100 - 100).toFixed(1)} % {speed > 1 ? 'faster' : 'slower'} than its own tempo, more than the stretch limit in the settings. The clip's panel offers to keep its own tempo.">{stretchBadge}</span>
  {/if}
  {#if broken}<span class="warn" title={clip.track?.problem ?? "track unavailable"}>{clip.track?.problem ?? "track unavailable"}</span>{/if}
  {#if clip.trimStart > SR * 0.05}<span class="cue" title="Cue point: audio before it is hidden"></span>{/if}
</div>

<style>
  /* the clip box only positions things; the pieces (.seg) take the pointer */
  .clip {
    position: absolute;
    top: 0;
    pointer-events: none;
  }
  .seg {
    position: absolute;
    top: 0;
    bottom: 0;
    box-sizing: border-box;
    pointer-events: auto;
    cursor: grab;
    outline: none;
    background: color-mix(in srgb, var(--c) 15%, var(--deep));
    border-top: 1px solid var(--c);
    border-bottom: 1px solid var(--c);
  }
  .seg.first {
    border-left: 2px solid var(--c);
    border-top-left-radius: 4px;
    border-bottom-left-radius: 4px;
  }
  .seg.last {
    border-right: 2px solid var(--c);
    border-top-right-radius: 4px;
    border-bottom-right-radius: 4px;
  }
  /* overlapped: see-through, so the clip underneath shows and the crossfade lines read over both */
  .seg.overlap {
    background: color-mix(in srgb, var(--c) 22%, transparent);
  }
  .handle {
    position: absolute;
    top: 0;
    width: 7px;
    /* above a crossfade (z-index 4) that ends or starts at this edge: the edge stays grabbable */
    z-index: 5;
    pointer-events: auto;
    cursor: ew-resize;
  }
  .handle.l {
    left: 0;
  }
  .handle.r {
    right: 0;
  }
  .seg:active {
    cursor: grabbing;
  }
  .seg.edge {
    cursor: ew-resize;
  }
  .selected .seg {
    border-color: #fff;
    box-shadow: inset 0 0 0 1px var(--c);
  }
  .seg:focus-visible {
    box-shadow: inset 0 0 0 2px var(--text);
  }
  .bypass .seg {
    opacity: 0.5;
  }
  .broken .seg {
    background: repeating-linear-gradient(135deg, var(--bad-bg) 0 8px, var(--deep) 8px 16px);
    border-color: var(--bad);
  }
  .title {
    position: absolute;
    left: 7px;
    font-size: 11px;
    font-weight: 600;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    color: var(--text);
    text-shadow: 0 0 3px #000, 0 0 3px #000, 0 0 5px #000;
    pointer-events: none;
    /* above the clips' tint and waveform (1-2), so a name running into an overlap is not painted over */
    z-index: 3;
  }
  .title::before {
    content: "";
    display: inline-block;
    width: 8px;
    height: 8px;
    margin-right: 5px;
    border-radius: 2px;
    background: var(--c);
  }
  .title em {
    margin-left: 8px;
    font-style: normal;
    font-weight: 400;
    color: var(--muted);
  }
  .stretch {
    position: absolute;
    left: 7px;
    font-size: 10px;
    font-weight: 700;
    color: #000;
    background: var(--warn);
    border-radius: 3px;
    padding: 0 5px;
    z-index: 2;
    pointer-events: none;
    white-space: nowrap;
  }
  .warn {
    position: absolute;
    left: 6px;
    bottom: 3px;
    font-size: 10px;
    color: var(--bad-text);
    z-index: 2;
    pointer-events: none;
  }
  .cue {
    position: absolute;
    left: 0;
    border-style: solid;
    border-width: 8px 8px 0 0;
    border-color: var(--green) transparent transparent transparent;
    pointer-events: none;
    z-index: 3;
  }
</style>
