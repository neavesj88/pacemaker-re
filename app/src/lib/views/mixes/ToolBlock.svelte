<script lang="ts">
  // One process (tool, loop, ignore or reverse region) of a clip, as a block on the tool lane.
  // Drag to move inside its clip, drag an edge to resize (a tool's envelopes stretch with it).
  import type { Clip, ProcessDoc } from "$lib/api/mixes";
  import { mixes } from "$lib/state/mixes.svelte";
  import { vocal } from "$lib/state/vocal.svelte";
  import { toolColour, toolLabel } from "./layout";
  import { vocalTools } from "./vocalTools.svelte";
  import { mv } from "./view.svelte";

  let {
    clip,
    process: p,
    top,
    height,
    conflict = false,
    accent = "var(--muted)",
    oncontext,
  }: {
    clip: Clip;
    process: ProcessDoc;
    top: number;
    height: number;
    conflict?: boolean;
    /** The colour of the clip the block belongs to, drawn as a stripe on its left edge. */
    accent?: string;
    oncontext: (e: MouseEvent, clip: Clip, p: ProcessDoc) => void;
  } = $props();

  const EDGE = 6;
  const type = $derived(p.kind === "tool" ? p.effects[0]?.name ?? p.toolType : p.kind);
  const left = $derived(mv.x(clip.start + p.start));
  const width = $derived(Math.max(4, mv.x(p.length)));
  const selected = $derived(mixes.selected.includes(p.id));
  const marked = $derived(p.kind === "tool" && mixes.doc ? vocalTools.has(mixes.doc.id, p.id) : false);
  const colour = $derived(toolColour(type));
  const label = $derived(p.kind === "loop" ? `Loop ×${p.iterations}` : toolLabel(type, vocal.mode, marked));

  /** The first parameter's curve, drawn inside the block as a hint (the editor is in the inspector). */
  const hint = $derived.by(() => {
    if (p.kind !== "tool" || width < 40) return "";
    const first = Object.values(p.params)[0];
    if (!first || first.length < 2) return "";
    const lo = Math.min(...first.map((n) => n.value));
    const hi = Math.max(...first.map((n) => n.value));
    if (hi - lo < 1e-9) return "";
    return first
      .map((n) => `${((n.pos / Math.max(1, p.length)) * width).toFixed(1)},${(height - 4 - ((n.value - lo) / (hi - lo)) * (height - 8)).toFixed(1)}`)
      .join(" ");
  });

  let hover = $state(false);
  function zone(e: PointerEvent | MouseEvent): "left" | "right" | "move" {
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const x = e.clientX - r.left;
    if (r.width > EDGE * 3) {
      if (x < EDGE) return "left";
      if (x > r.width - EDGE) return "right";
    }
    return "move";
  }

  function down(e: PointerEvent) {
    if (e.button !== 0) return;
    e.stopPropagation();
    if (e.shiftKey || e.ctrlKey || e.metaKey) {
      mixes.toggleSelected(p.id);
      return;
    }
    mixes.select([p.id]);
    const z = zone(e);
    const startX = e.clientX;
    const start0 = p.start;
    const end0 = p.start + p.length;
    let moved = false;
    const onmove = (ev: PointerEvent) => {
      if (!moved && Math.abs(ev.clientX - startX) < 3) return;
      moved = true;
      const d = mv.samples(ev.clientX - startX);
      const alt = ev.altKey;
      const keep = mixes.snap;
      if (alt) mixes.snap = "off";
      if (z === "move") mixes.moveProcess(clip.id, p.id, start0 + d, { live: true });
      else if (z === "left") mixes.resizeProcess(clip.id, p.id, "left", start0 + d, { live: true });
      else mixes.resizeProcess(clip.id, p.id, "right", end0 + d, { live: true });
      mixes.snap = keep;
    };
    const up = () => {
      window.removeEventListener("pointermove", onmove);
      window.removeEventListener("pointerup", up);
      window.removeEventListener("pointercancel", up);
      mixes.endGesture();
    };
    window.addEventListener("pointermove", onmove);
    window.addEventListener("pointerup", up);
    window.addEventListener("pointercancel", up);
  }

  function ctx(e: MouseEvent) {
    e.preventDefault();
    e.stopPropagation();
    if (!mixes.selected.includes(p.id)) mixes.select([p.id]);
    oncontext(e, clip, p);
  }
</script>

<div
  class="block"
  class:selected
  class:bypassed={p.bypassed}
  class:conflict
  class:edge={hover}
  role="button"
  tabindex="0"
  aria-label="{label} on {clip.track?.title ?? 'clip'}"
  aria-pressed={selected}
  title={conflict ? `${label}: overlaps another ${label} on this clip (saving is refused)` : label}
  style:left="{left}px"
  style:width="{width}px"
  style:top="{top}px"
  style:height="{height}px"
  style:background={colour}
  style:--accent-clip={accent}
  onpointerdown={down}
  onpointermove={(e) => (hover = zone(e) !== "move")}
  onpointerleave={() => (hover = false)}
  oncontextmenu={ctx}
  onkeydown={(e) => {
    // Space is left to the window handler (play / pause)
    if (e.key === "Enter") {
      e.preventDefault();
      mixes.select([p.id]);
    }
  }}
>
  {#if hint}
    <svg class="hint" {width} {height} aria-hidden="true"><polyline points={hint} fill="none" stroke="rgba(0,0,0,.55)" stroke-width="1.5" /></svg>
  {/if}
  <span class="label">{label}</span>
</div>

<style>
  .block {
    position: absolute;
    box-sizing: border-box;
    border-radius: 4px;
    overflow: hidden;
    color: #000;
    font-size: 11px;
    font-weight: 600;
    cursor: grab;
    outline: none;
    border: 1px solid rgba(0, 0, 0, 0.35);
    border-left: 4px solid var(--accent-clip);
  }
  .block.edge {
    cursor: ew-resize;
  }
  .block.selected {
    outline: 2px solid #fff;
    outline-offset: -1px;
  }
  .block:focus-visible {
    outline: 2px solid var(--text);
  }
  .block.bypassed {
    opacity: 0.4;
  }
  .block.conflict {
    box-shadow: 0 0 0 2px var(--bad);
  }
  .label {
    position: absolute;
    left: 5px;
    top: 50%;
    transform: translateY(-50%);
    white-space: nowrap;
    pointer-events: none;
  }
  .hint {
    position: absolute;
    inset: 0;
    pointer-events: none;
  }
</style>
