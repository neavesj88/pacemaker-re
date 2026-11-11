<script lang="ts">
  // One automation envelope as an editable SVG: click on empty space adds a node, drag moves it
  // (between its neighbours), right-click or Delete removes it. The same component draws the
  // lanes of the timeline (a clip's gain / key / tempo) and the parameter curves of the
  // inspector (a tool's parameters, a crossfade curve).
  //
  // Nodes are { pos, value } in the owner's units; the visible range of positions is [d0, d1] and
  // maps onto `width`. Curves are flat outside their first and last node.
  import type { EnvNode } from "$lib/api/mixes";
  import { envelopeAt, insertNode, moveNode, removeNode } from "$lib/state/mixedit";
  import { fmtValue } from "./layout";

  let {
    nodes,
    d0,
    d1,
    min,
    max,
    width,
    height,
    colour = "var(--accent)",
    editable = true,
    lockEnds = false,
    minNodes = 1,
    snapValue,
    snapPos,
    guides = [],
    unit = "",
    label = "Envelope",
    lineDrag,
    onchange,
    ondone,
    selected = $bindable(-1),
    onselect,
  }: {
    nodes: EnvNode[];
    d0: number;
    d1: number;
    min: number;
    max: number;
    width: number;
    height: number;
    colour?: string;
    editable?: boolean;
    /** First and last node keep their position (a tool's envelope spans its whole block). */
    lockEnds?: boolean;
    minNodes?: number;
    snapValue?: (v: number) => number;
    snapPos?: (p: number) => number;
    /** Faint horizontal guide lines at these values (e.g. 0 dB). */
    guides?: number[];
    unit?: string;
    label?: string;
    /** When given, dragging anywhere moves the whole (single-node) curve vertically. */
    lineDrag?: (value: number, live: boolean) => void;
    onchange?: (nodes: EnvNode[], live: boolean) => void;
    ondone?: () => void;
    selected?: number;
    onselect?: (index: number) => void;
  } = $props();

  const PAD = 7;
  const span = $derived(Math.max(1, d1 - d0));
  const px = (pos: number) => ((pos - d0) / span) * width;
  const py = (v: number) => PAD + (1 - (v - min) / (max - min || 1)) * Math.max(1, height - 2 * PAD);
  const posAt = (x: number) => d0 + (x / Math.max(1, width)) * span;
  const valueAt = (y: number) => {
    const v = min + (1 - (y - PAD) / Math.max(1, height - 2 * PAD)) * (max - min);
    const c = Math.min(max, Math.max(min, v));
    return snapValue ? Math.min(max, Math.max(min, snapValue(c))) : c;
  };

  /** Points of the line: the value at the left edge, every node inside, the value at the right edge. */
  const line = $derived.by(() => {
    const pts: string[] = [`0,${py(envelopeAt(nodes, d0)).toFixed(1)}`];
    for (const n of nodes) {
      if (n.pos > d0 && n.pos < d1) pts.push(`${px(n.pos).toFixed(1)},${py(n.value).toFixed(1)}`);
    }
    pts.push(`${width.toFixed(1)},${py(envelopeAt(nodes, d1)).toFixed(1)}`);
    return pts.join(" ");
  });

  const visible = $derived(nodes.map((n, i) => ({ n, i })).filter(({ n }) => n.pos >= d0 && n.pos <= d1));

  let svg = $state<SVGSVGElement | null>(null);
  let active = $state(-1);
  let drag: { index: number; lineBase?: boolean } | null = null;

  function local(e: PointerEvent): { x: number; y: number } {
    const r = svg!.getBoundingClientRect();
    return { x: e.clientX - r.left, y: e.clientY - r.top };
  }

  function select(i: number) {
    selected = i;
    onselect?.(i);
  }

  function down(e: PointerEvent) {
    if (!editable || e.button !== 0 || !svg) return;
    const target = e.target as SVGElement;
    const idx = target.dataset.node !== undefined ? Number(target.dataset.node) : -1;
    const { x, y } = local(e);
    svg.setPointerCapture(e.pointerId);
    svg.focus();
    if (lineDrag && idx < 0) {
      drag = { index: -1, lineBase: true };
      lineDrag(valueAt(y), true);
      return;
    }
    if (idx >= 0) {
      drag = { index: idx };
      active = idx;
      select(idx);
    } else {
      const pos = Math.round(snapPos ? snapPos(posAt(x)) : posAt(x));
      const next = insertNode(nodes, { pos, value: valueAt(y) });
      const i = next.findIndex((n) => n.pos === pos);
      onchange?.(next, true);
      drag = { index: i };
      active = i;
      select(i);
    }
    e.preventDefault();
  }

  function move(e: PointerEvent) {
    if (!drag) return;
    const { x, y } = local(e);
    if (drag.lineBase) {
      lineDrag?.(valueAt(y), true);
      return;
    }
    const i = drag.index;
    const fixed = lockEnds && (i === 0 || i === nodes.length - 1);
    const change: Partial<EnvNode> = { value: valueAt(y) };
    if (!fixed) change.pos = Math.round(snapPos ? snapPos(posAt(x)) : posAt(x));
    onchange?.(moveNode(nodes, i, change), true);
  }

  function up(_e: PointerEvent) {
    if (!drag) return;
    // every move already edited the document inside one undo group; this closes the group
    drag = null;
    active = -1;
    ondone?.();
  }

  function removeAt(i: number) {
    if (!editable || nodes.length <= minNodes) return;
    if (lockEnds && (i === 0 || i === nodes.length - 1)) return;
    onchange?.(removeNode(nodes, i), false);
    select(-1);
    ondone?.();
  }

  function ctx(e: MouseEvent) {
    const t = e.target as SVGElement;
    if (t.dataset.node !== undefined) {
      e.preventDefault();
      e.stopPropagation();
      removeAt(Number(t.dataset.node));
    }
  }

  function key(e: KeyboardEvent) {
    if ((e.key === "Delete" || e.key === "Backspace") && selected >= 0) {
      e.preventDefault();
      e.stopPropagation();
      removeAt(selected);
    }
  }
</script>

<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<svg
  bind:this={svg}
  class="env"
  class:editable
  {width}
  {height}
  role="application"
  aria-label={label}
  tabindex={editable ? 0 : -1}
  onpointerdown={down}
  onpointermove={move}
  onpointerup={up}
  onpointercancel={up}
  oncontextmenu={ctx}
  onkeydown={key}
>
  {#each guides as g (g)}
    <line class="guide" x1="0" x2={width} y1={py(g)} y2={py(g)} />
  {/each}
  <polyline class="curve" points={line} stroke={colour} fill="none" />
  {#if editable}
    {#each visible as { n, i } (i)}
      <circle
        class="node"
        class:sel={selected === i}
        data-node={i}
        cx={px(n.pos)}
        cy={py(n.value)}
        r={selected === i || active === i ? 5.5 : 4}
        stroke={colour}
      />
    {/each}
    {#if active >= 0 && nodes[active]}
      <text class="val" x={Math.min(width - 4, Math.max(4, px(nodes[active].pos) + 8))} y={Math.max(11, py(nodes[active].value) - 8)}>
        {fmtValue(nodes[active].value, unit)}
      </text>
    {/if}
  {/if}
</svg>

<style>
  .env {
    display: block;
    touch-action: none;
    outline: none;
  }
  .editable {
    cursor: crosshair;
  }
  .editable:focus-visible {
    outline: 1px solid var(--line-hi);
  }
  .curve {
    stroke-width: 2;
    stroke-linejoin: round;
    pointer-events: none;
  }
  .guide {
    stroke: var(--line);
    stroke-width: 1;
    stroke-dasharray: 3 4;
    pointer-events: none;
  }
  .node {
    fill: #000;
    stroke-width: 2;
    cursor: grab;
  }
  .node.sel {
    fill: var(--text);
  }
  .val {
    fill: var(--text);
    font-size: 11px;
    paint-order: stroke;
    stroke: #000;
    stroke-width: 3px;
    pointer-events: none;
  }
</style>
