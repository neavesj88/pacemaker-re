<script lang="ts">
  // Inspector for one tool (with a curve editor per parameter), loop, ignore or reverse region.
  import type { Clip, EnvNode, LoopDoc, ProcessDoc, ToolDoc } from "$lib/api/mixes";
  import * as E from "$lib/state/mixedit";
  import { mixes } from "$lib/state/mixes.svelte";
  import EnvelopeSvg from "./EnvelopeSvg.svelte";
  import { clipTitle, duplicateSelection } from "./actions";
  import { SR, clock, fmtValue, toolColour, toolLabel } from "./layout";

  let { clip, process: p, ondelete }: { clip: Clip; process: ProcessDoc; ondelete: () => void } = $props();

  const type = $derived(p.kind === "tool" ? p.effects[0]?.name ?? p.toolType : p.kind);
  const tool = $derived(p.kind === "tool" ? (p as ToolDoc) : null);
  const catalogue = $derived(E.TOOL_TYPES.find((t) => t.name === type));
  const clash = $derived(tool ? E.toolConflicts(clip, tool).length > 0 : false);
  let selNode = $state(-1);

  /** Parameters to edit: the catalogue's (known ranges), else whatever keys the tool carries. */
  const params = $derived.by(() => {
    if (!tool) return [];
    const known = catalogue?.params ?? [];
    const keys = Object.keys(tool.params);
    const out = known.filter((k) => keys.includes(k.key));
    for (const key of keys) {
      if (out.some((k) => k.key === key)) continue;
      const vals = tool.params[key].map((n) => n.value);
      const lo = Math.min(0, ...vals);
      const hi = Math.max(1, ...vals);
      out.push({ key, label: key.replace(/^[^-]*-/, "").trim() || key, min: lo, max: hi, unit: "", flat: vals[0] ?? 0 });
    }
    return out;
  });

  let boxW = $state(250);

  function setNodes(key: string, nodes: EnvNode[]) {
    mixes.setEnvelope(clip.id, { kind: "param", processId: p.id, key }, nodes, { live: true });
  }
  function flatten(key: string, to: number) {
    const last = Math.max(1, Math.round(p.length) - 1);
    mixes.setEnvelope(clip.id, { kind: "param", processId: p.id, key }, [{ pos: 0, value: to }, { pos: last, value: to }]);
  }
  function setStart(sec: number) {
    mixes.edit("Move tool", (d) => E.moveProcess(d, clip.id, p.id, Math.round(sec * SR)));
  }
  function setLength(sec: number) {
    mixes.edit("Resize tool", (d) => E.resizeProcess(d, clip.id, p.id, "right", p.start + Math.round(sec * SR)));
  }
  function patch(label: string, fn: (q: ProcessDoc) => ProcessDoc) {
    mixes.updateProcess(clip.id, p.id, label, fn);
  }
  const step = (max: number, min: number) => ((max - min) > 50 ? 1 : (max - min) > 5 ? 0.1 : 0.01);
</script>

<h3 style:color={toolColour(type)}>{p.kind === "loop" ? "Loop" : toolLabel(type)}</h3>
<div class="muted">{p.kind === "tool" ? "tool" : "region"} on “{clipTitle(clip)}”</div>

{#if clash}
  <div class="warn">Another {toolLabel(type)} overlaps this one on the same clip. Saving is refused until they are separate: move or shorten one.</div>
{/if}

<div class="field">
  <span class="lbl">Active</span>
  <label class="check"><input type="checkbox" checked={!p.bypassed} onchange={(e) => patch(e.currentTarget.checked ? "Enable" : "Mute", (q) => ({ ...q, bypassed: !e.currentTarget.checked }))} /> {p.bypassed ? "muted" : "on"}</label>
</div>
<div class="field">
  <label for="pp-start">Starts at</label>
  <span class="inline">
    <input id="pp-start" type="number" min="0" step="0.01" value={Number((p.start / SR).toFixed(2))} onchange={(e) => setStart(e.currentTarget.valueAsNumber)} />
    <span class="muted">s</span>
  </span>
</div>
<div class="field">
  <label for="pp-len">Length</label>
  <span class="inline">
    <input id="pp-len" type="number" min="0.05" step="0.01" value={Number((p.length / SR).toFixed(2))} onchange={(e) => setLength(e.currentTarget.valueAsNumber)} />
    <span class="muted">s</span>
  </span>
</div>
<div class="muted small">on the clip: {clock(p.start / SR)} to {clock((p.start + p.length) / SR)}</div>

{#if p.kind === "loop"}
  {@const lp = p as LoopDoc}
  <h4>Loop</h4>
  <div class="field">
    <label for="pp-iter">Repeats</label>
    <input id="pp-iter" type="number" min="1" max="64" step="1" value={lp.iterations} onchange={(e) => patch("Loop repeats", (q) => ({ ...q, iterations: Math.max(1, Math.round(e.currentTarget.valueAsNumber || 1)) }) as LoopDoc)} />
  </div>
  <div class="field"><span class="lbl">Slice</span><span class="val">{clock(lp.iterationLength / SR)}</span></div>
  <div class="field">
    <span class="lbl">Re-loop</span>
    <label class="check"><input type="checkbox" checked={lp.reLoop} onchange={(e) => patch("Re-loop", (q) => ({ ...q, reLoop: e.currentTarget.checked }) as LoopDoc)} /> repeat again</label>
  </div>
  {#if lp.parent}<div class="muted small">This loop continues the previous loop of the clip.</div>{/if}
{/if}

{#if tool}
  <h4>Parameters</h4>
  <div class="curves" bind:clientWidth={boxW}>
    {#each params as prm (prm.key)}
      {@const nodes = tool.params[prm.key]}
      <div class="param">
        <div class="phead">
          <span>{prm.label}</span>
          <span class="muted">{fmtValue(prm.min)}…{fmtValue(prm.max)} {prm.unit}</span>
          <button class="small-btn" onclick={() => flatten(prm.key, prm.flat)} title="Reset this curve to its neutral value">Reset</button>
        </div>
        <div class="curvebox">
          <EnvelopeSvg
            {nodes}
            d0={0}
            d1={Math.max(1, Math.round(tool.length) - 1)}
            min={prm.min}
            max={prm.max}
            width={Math.max(120, boxW - 2)}
            height={78}
            colour={toolColour(type)}
            lockEnds
            minNodes={2}
            guides={[prm.flat]}
            unit={prm.unit}
            label="{prm.label} curve"
            snapValue={(v) => Math.round(v / step(prm.max, prm.min)) * step(prm.max, prm.min)}
            onchange={(n) => setNodes(prm.key, n)}
            ondone={() => mixes.endGesture()}
            bind:selected={selNode}
          />
        </div>
      </div>
    {/each}
  </div>
  <p class="note">Click on a curve to add a node, drag to move, right-click a node (or select it and press Delete) to remove it. The first and last node stay at the ends of the block.</p>
{/if}

<div class="buttons">
  <button onclick={duplicateSelection}>Duplicate</button>
  <button onclick={() => mixes.copy()}>Copy</button>
  <button class="danger" onclick={ondelete}>Delete…</button>
</div>

<style>
  .small {
    font-size: 12px;
    margin: 2px 0 6px;
  }
  .lbl {
    color: var(--muted);
    font-size: 12px;
  }
  .inline {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
  }
  .inline input {
    flex: 1;
    width: auto;
    min-width: 0;
  }
  .check {
    display: flex;
    gap: 6px;
    align-items: center;
    color: var(--text);
    font-size: 13px;
  }
  .curves {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .phead {
    display: flex;
    gap: 8px;
    align-items: baseline;
    font-size: 12px;
    margin-bottom: 3px;
  }
  .phead .muted {
    margin-left: auto;
  }
  .small-btn {
    padding: 0 6px;
    font-size: 11px;
  }
</style>
