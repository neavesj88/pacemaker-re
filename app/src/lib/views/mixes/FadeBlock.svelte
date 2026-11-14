<script lang="ts">
  // A crossfade drawn ON the overlap of two clips: the incoming clip's fader curve in its colour,
  // the outgoing clip's (the inverse) in its colour, nodes draggable right there. A click on
  // empty space adds a node, a right-click or double-click on a node removes it. The strip at the
  // top moves the crossfade, its edges resize it. The first press on a crossfade that is not
  // selected only selects it.
  import type { EnvNode, TransitionDoc } from "$lib/api/mixes";
  import * as E from "$lib/state/mixedit";
  import { mixes } from "$lib/state/mixes.svelte";
  import EnvelopeSvg from "./EnvelopeSvg.svelte";
  import { CURVE_SCALE, curveAt, curveToEnv, envToCurve } from "./layout";
  import { mv } from "./view.svelte";

  let {
    t,
    height,
    fromColour,
    toColour,
    oncontext,
  }: {
    t: TransitionDoc;
    /** Height of the whole bar. */
    height: number;
    /** Colour of the outgoing clip and of the incoming one. */
    fromColour: string;
    toColour: string;
    oncontext: (e: MouseEvent, t: TransitionDoc) => void;
  } = $props();

  const EDGE = 7;
  const HEAD = 16;
  const left = $derived(mv.x(t.start));
  const width = $derived(Math.max(8, mv.x(t.length)));
  const selected = $derived(mixes.selected.includes(t.id));
  const bodyH = $derived(Math.max(20, height - HEAD));
  const outgoing = $derived.by(() => {
    const n = Math.max(2, Math.min(60, Math.round(width / 6)));
    const pts: string[] = [];
    for (let i = 0; i <= n; i++) {
      const x = i / n;
      pts.push(`${(x * width).toFixed(1)},${(7 + curveAt(t.curve, x) * (bodyH - 14)).toFixed(1)}`);
    }
    return pts.join(" ");
  });
  const fromTitle = $derived(t.between ? (E.clipOf(mixes.doc!, t.between.from)?.track?.title ?? "") : "");
  let selNode = $state(-1);

  function selectFirst(e: PointerEvent) {
    if (e.button !== 0 || e.shiftKey || e.ctrlKey || e.metaKey || selected) return;
    mixes.select([t.id]);
    e.stopPropagation(); // this press only selects; the next one edits
  }

  function edgeAt(e: PointerEvent | MouseEvent): "left" | "right" | "move" {
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const x = e.clientX - r.left;
    if (r.width > EDGE * 3) {
      if (x < EDGE) return "left";
      if (x > r.width - EDGE) return "right";
    }
    return "move";
  }

  /** The strip (move) and the edges (resize) are the handles; both are plain pointer drags. */
  function grab(e: PointerEvent, z: "left" | "right" | "move") {
    if (e.button !== 0) return;
    e.stopPropagation();
    if (e.shiftKey || e.ctrlKey || e.metaKey) {
      mixes.toggleSelected(t.id);
      return;
    }
    mixes.select([t.id]);
    const startX = e.clientX;
    const { start, length } = t;
    let moved = false;
    const onmove = (ev: PointerEvent) => {
      if (!moved && Math.abs(ev.clientX - startX) < 3) return;
      moved = true;
      const d = mv.samples(ev.clientX - startX);
      const opts = { mode: ev.altKey ? ("off" as const) : mixes.snap, threshold: mixes.threshold, sections: mixes.sectionLookup };
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

  function setCurve(nodes: EnvNode[]) {
    mixes.setCurve(t.id, envToCurve(nodes), { live: true });
  }

  function ctx(e: MouseEvent) {
    e.preventDefault();
    e.stopPropagation();
    if (!mixes.selected.includes(t.id)) mixes.select([t.id]);
    oncontext(e, t);
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="xf"
  class:selected
  class:bypassed={t.bypassed}
  data-crossfade={t.id}
  style:left="{left}px"
  style:width="{width}px"
  style:height="{height}px"
  style:--out={fromColour}
  style:--in={toColour}
  aria-label="Crossfade{fromTitle ? ' from ' + fromTitle : ''}"
  onpointerdowncapture={selectFirst}
  oncontextmenu={ctx}
>
  <svg class="outline" {width} height={bodyH} style:top="{HEAD}px" aria-hidden="true">
    <polyline points={outgoing} fill="none" stroke={fromColour} stroke-width="2" stroke-dasharray="5 3" />
  </svg>
  <div class="env" style:top="{HEAD}px">
    <EnvelopeSvg
      nodes={curveToEnv(t.curve)}
      d0={0}
      d1={CURVE_SCALE}
      min={0}
      max={1}
      {width}
      height={bodyH}
      colour={toColour}
      guides={[0, 1]}
      label="Crossfade curve"
      lockEnds
      minNodes={2}
      editable={selected}
      snapValue={(v) => Math.round(v * 100) / 100}
      onchange={(n) => setCurve(n)}
      ondone={() => mixes.endGesture()}
      bind:selected={selNode}
    />
  </div>
  <div
    class="strip"
    role="button"
    tabindex="-1"
    aria-label="Move the crossfade"
    title="Drag to move the crossfade; drag an edge to resize it. Click the curve to add a node, right-click or double-click a node to remove it."
    style:height="{HEAD}px"
    onpointerdown={(e) => grab(e, edgeAt(e))}
    onpointermove={(e) => ((e.currentTarget as HTMLElement).style.cursor = edgeAt(e) === "move" ? "grab" : "ew-resize")}
  >
    {#if width > 56}<span>X-fade{t.beatMatch ? " · beat-matched" : ""}</span>{/if}
  </div>
</div>

<style>
  .xf {
    position: absolute;
    top: 0;
    box-sizing: border-box;
    border: 1px dashed color-mix(in srgb, var(--text) 55%, transparent);
    border-radius: 4px;
    background: rgba(0, 0, 0, 0.25);
    z-index: 4;
    overflow: hidden;
  }
  .xf.selected {
    border: 1px solid #fff;
    box-shadow: 0 0 0 1px var(--in);
    background: rgba(0, 0, 0, 0.38);
  }
  .xf.bypassed {
    opacity: 0.45;
  }
  .outline,
  .env {
    position: absolute;
    left: 0;
  }
  .outline {
    pointer-events: none;
  }
  .strip {
    position: absolute;
    left: 0;
    right: 0;
    top: 0;
    background: color-mix(in srgb, var(--in) 38%, #000);
    cursor: grab;
    display: flex;
    align-items: center;
    padding: 0 8px;
    font-size: 10px;
    font-weight: 700;
    color: var(--text);
    white-space: nowrap;
    overflow: hidden;
  }
</style>
