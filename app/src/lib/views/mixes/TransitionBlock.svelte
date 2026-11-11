<script lang="ts">
  // A crossfade on the transitions lane, drawn with its fader curve (the earlier clip fades
  // out along the inverse). Drag to move, drag an edge to resize; the curve is edited in the
  // inspector.
  import type { TransitionDoc } from "$lib/api/mixes";
  import * as E from "$lib/state/mixedit";
  import { mixes } from "$lib/state/mixes.svelte";
  import { curveAt } from "./layout";
  import { mv } from "./view.svelte";

  let { t, height, oncontext }: { t: TransitionDoc; height: number; oncontext: (e: MouseEvent, t: TransitionDoc) => void } = $props();

  const EDGE = 6;
  const left = $derived(mv.x(t.start));
  const width = $derived(Math.max(6, mv.x(t.length)));
  const selected = $derived(mixes.selected.includes(t.id));
  const pts = $derived.by(() => {
    const n = Math.max(2, Math.min(48, Math.round(width / 6)));
    const up: string[] = [];
    const down: string[] = [];
    for (let i = 0; i <= n; i++) {
      const x = i / n;
      const v = curveAt(t.curve, x);
      up.push(`${(x * width).toFixed(1)},${(height - 4 - v * (height - 8)).toFixed(1)}`);
      down.push(`${(x * width).toFixed(1)},${(4 + v * (height - 8)).toFixed(1)}`);
    }
    return { up: up.join(" "), down: down.join(" ") };
  });
  const fromTitle = $derived(t.between ? (E.clipOf(mixes.doc!, t.between.from)?.track?.title ?? "") : "");

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
      mixes.toggleSelected(t.id);
      return;
    }
    mixes.select([t.id]);
    const z = zone(e);
    const startX = e.clientX;
    const { start, length } = t;
    let moved = false;
    const onmove = (ev: PointerEvent) => {
      if (!moved && Math.abs(ev.clientX - startX) < 3) return;
      moved = true;
      const d = mv.samples(ev.clientX - startX);
      const opts = { mode: ev.altKey ? ("off" as const) : mixes.snap, threshold: mixes.threshold };
      mixes.edit(
        "Move crossfade",
        (doc) => {
          if (z === "move") return E.moveTransition(doc, t.id, start + d, length, opts);
          if (z === "left") {
            const s = Math.min(start + d, start + length - E.MIN_LENGTH);
            return E.moveTransition(doc, t.id, s, start + length - s, opts);
          }
          return E.moveTransition(doc, t.id, start, Math.max(E.MIN_LENGTH, length + d), opts);
        },
        { group: `tmove:${t.id}:${z}` },
      );
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
    if (!mixes.selected.includes(t.id)) mixes.select([t.id]);
    oncontext(e, t);
  }
</script>

<div
  class="xf"
  class:selected
  class:bypassed={t.bypassed}
  class:edge={hover}
  role="button"
  tabindex="0"
  aria-label="Crossfade{fromTitle ? ' from ' + fromTitle : ''}"
  aria-pressed={selected}
  style:left="{left}px"
  style:width="{width}px"
  style:height="{height}px"
  onpointerdown={down}
  onpointermove={(e) => (hover = zone(e) !== "move")}
  onpointerleave={() => (hover = false)}
  oncontextmenu={ctx}
  onkeydown={(e) => {
    if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      mixes.select([t.id]);
    }
  }}
>
  <svg {width} {height} aria-hidden="true">
    <polyline points={pts.down} fill="none" stroke="rgba(255,255,255,.6)" stroke-width="1.5" />
    <polyline points={pts.up} fill="none" stroke="var(--accent)" stroke-width="2" />
  </svg>
  {#if width > 70}<span class="label">X-fade{t.beatMatch ? " · beat-matched" : ""}</span>{/if}
</div>

<style>
  .xf {
    position: absolute;
    top: 4px;
    box-sizing: border-box;
    border: 1px solid var(--line-hi);
    border-radius: 4px;
    background: rgba(255, 0, 200, 0.1);
    overflow: hidden;
    cursor: grab;
    outline: none;
  }
  .xf.edge {
    cursor: ew-resize;
  }
  .xf.selected {
    border-color: var(--accent);
    box-shadow: 0 0 0 1px var(--accent);
    background: rgba(255, 0, 200, 0.2);
  }
  .xf:focus-visible {
    border-color: var(--text);
  }
  .xf.bypassed {
    opacity: 0.45;
  }
  svg {
    position: absolute;
    inset: 0;
    pointer-events: none;
  }
  .label {
    position: absolute;
    left: 6px;
    top: 2px;
    font-size: 10px;
    color: var(--text);
    text-shadow: 0 0 3px #000;
    pointer-events: none;
    white-space: nowrap;
  }
</style>
