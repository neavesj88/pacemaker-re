<script lang="ts">
  // The timeline: a ruler, ONE mix bar and the tool strip under it, with the playhead over both.
  //
  // Every clip is on the same bar in its own colour. Where two clips overlap, the bar is split by
  // height inside the overlap (see ClipBlock) and the crossfade is drawn on the overlap itself
  // (FadeBlock). Tools, loops, reverse and ignore blocks sit in a thin strip attached under the bar,
  // at the same horizontal position as the clip they belong to, with the clip's colour on their
  // edge. One of the clips' envelopes (gain, tempo or key) can be drawn over the bar.
  //
  // Only what is near the viewport is rendered (clips, blocks, ruler marks, waveform canvases),
  // so a long mix with many clips stays as cheap as a short one.
  import type { Clip, EnvNode, ProcessDoc, TransitionDoc } from "$lib/api/mixes";
  import * as E from "$lib/state/mixedit";
  import { mixes } from "$lib/state/mixes.svelte";
  import { laneOffset } from "$lib/state/dragLogic";
  import { trackDrag, trackDropTarget, type DropOptions } from "$lib/state/trackDrag.svelte";
  import ContextMenu, { type MenuItem } from "$lib/ui/ContextMenu.svelte";
  import { addProcessTo, addTracks, autoTransition, beatmatch, clipTitle, duplicateSelection, neighbours, pasteAtPlayhead, targetClip, toolChoices } from "./actions";
  import ClipBlock from "./ClipBlock.svelte";
  import EnvelopeSvg from "./EnvelopeSvg.svelte";
  import FadeBlock from "./FadeBlock.svelte";
  import { toolPrefs } from "./listPrefs.svelte";
  import { HEADER_W, PPS_MAX, PPS_MIN, SR, clamp, clipColour, envToKey, keyToEnv, packRows, rulerTicks, mmss } from "./layout";
  import { preview } from "./preview.svelte";
  import ToolBlock from "./ToolBlock.svelte";
  import { mv } from "./view.svelte";

  let { ondelete }: { ondelete: () => void } = $props();

  const RULER_H = 40;
  /** The bar: padding, then the clips (and the crossfades over their overlaps). It takes the height
   * the pane has left, between a readable minimum and a maximum. */
  const BAR_MIN = 190;
  const BAR_MAX = 380;
  const PAD = 6;
  const FX_ROW = 24;
  const ENV_TOP = 18;

  const doc = $derived(mixes.doc!);
  let scroller = $state<HTMLDivElement | null>(null);
  let root = $state<HTMLDivElement | null>(null);
  let barEl = $state<HTMLDivElement | null>(null);
  let menu = $state<{ x: number; y: number; items: MenuItem[] } | null>(null);

  // snap distance follows the zoom: about 10 px
  $effect(() => {
    mixes.threshold = Math.max(1, Math.round(mv.samples(10)));
  });

  // ---- geometry -----------------------------------------------------------------------------

  const bpm = $derived(doc.tempoBpm ?? (doc.clips[0] ? E.effectiveBpm(doc.clips[0], doc.clips[0].trimStart) : 120));
  const contentW = $derived(Math.max(mv.viewW - HEADER_W, mv.x(Math.max(doc.length, mv.playhead) + 45 * SR)));
  /** Visible range in track pixels, with half a screen of margin each side. */
  const vis = $derived({ l: mv.scrollLeft - mv.viewW / 2, r: mv.scrollLeft + mv.viewW * 1.5 });
  const inView = (startSamples: number, endSamples: number) => mv.x(endSamples) >= vis.l && mv.x(startSamples) <= vis.r;

  const ticks = $derived(rulerTicks(Math.max(0, vis.l / mv.pps), Math.max(0, vis.r / mv.pps), mv.pps, bpm));

  /** A clip's colour: by the order the clips were added, so it never changes when a clip moves. */
  const colours = $derived(new Map(doc.clips.map((c, i) => [c.id, clipColour(i)])));
  const colourOf = (id: string | undefined) => (id ? (colours.get(id) ?? "var(--accent)") : "var(--accent)");

  const visibleClips = $derived(doc.clips.filter((c) => inView(c.visibleStart, c.visibleEnd)));

  /** The tool strip: every process of every clip, packed into rows so none hides another. */
  const fx = $derived.by(() => {
    const items: { clip: Clip; p: ProcessDoc; start: number; end: number }[] = [];
    for (const clip of doc.clips) for (const p of clip.processes) items.push({ clip, p, start: clip.start + p.start, end: clip.start + p.start + p.length });
    const { rows, count } = packRows(items);
    return { items: items.map((it, i) => ({ ...it, row: rows[i] })), count: items.length === 0 ? 1 : count };
  });
  const fxH = $derived(fx.count * (FX_ROW + 2) + 8);
  let viewH = $state(0);
  const BAR_H = $derived(Math.min(BAR_MAX, Math.max(BAR_MIN, viewH - RULER_H - Math.max(40, fxH) - 2)));
  const LANE_H = $derived(BAR_H - PAD * 2);
  const conflicts = $derived.by(() => {
    const out = new Set<string>();
    for (const c of doc.clips) {
      for (const p of c.processes) if (p.kind === "tool") for (const id of E.toolConflicts(c, p)) out.add(id);
    }
    return out;
  });

  const visibleTransitions = $derived(doc.transitions.filter((t) => inView(t.start, t.start + t.length)));

  /** Pairs of clips that overlap with no crossfade yet. */
  const gaps = $derived.by(() => {
    const out: { from: Clip; to: Clip; start: number; end: number }[] = [];
    const sorted = E.sortedClips(doc);
    for (let i = 0; i < sorted.length; i++) {
      for (let j = i + 1; j < sorted.length; j++) {
        const a = sorted[i];
        const b = sorted[j];
        if (b.visibleStart >= a.visibleEnd) break;
        if (a.channel === b.channel) continue;
        const start = b.visibleStart;
        const end = Math.min(a.visibleEnd, b.visibleEnd);
        if (end - start < SR * 0.25) continue;
        const has = doc.transitions.some((t) =>
          t.between ? (t.between.from === a.id && t.between.to === b.id) || (t.between.from === b.id && t.between.to === a.id) : t.start < end && t.start + t.length > start,
        );
        if (!has) out.push({ from: a, to: b, start, end });
      }
    }
    return out;
  });

  // ---- scrolling and zoom -------------------------------------------------------------------

  function onscroll() {
    if (scroller) {
      mv.scrollLeft = scroller.scrollLeft;
      mv.viewW = scroller.clientWidth;
    }
  }

  // zoom and "scroll to" set mv.scrollLeft; mirror it to the element
  $effect(() => {
    const s = scroller;
    const want = mv.scrollLeft;
    if (s && Math.abs(s.scrollLeft - want) > 0.5) s.scrollLeft = want;
  });

  $effect(() => {
    const s = scroller;
    if (!s) return;
    const wheel = (e: WheelEvent) => {
      if (e.ctrlKey || e.metaKey) {
        e.preventDefault();
        const r = s.getBoundingClientRect();
        mv.setZoom(clamp(mv.pps * Math.exp(-e.deltaY * 0.0015), PPS_MIN, PPS_MAX), e.clientX - r.left - HEADER_W);
      } else if (e.shiftKey && e.deltaX === 0) {
        e.preventDefault();
        s.scrollLeft += e.deltaY;
      }
    };
    s.addEventListener("wheel", wheel, { passive: false });
    const size = () => {
      mv.viewW = s.clientWidth;
      viewH = s.clientHeight;
    };
    const ro = new ResizeObserver(size);
    ro.observe(s);
    size();
    return () => {
      s.removeEventListener("wheel", wheel);
      ro.disconnect();
    };
  });

  // keep the playhead in view while playing: scroll when it runs off the right edge
  $effect(() => {
    const px = mv.x(mv.playhead);
    if (!preview.playing || !mv.follow) return;
    const right = mv.scrollLeft + mv.viewW - HEADER_W;
    if (px > right - 24 && px < right + 400) mv.scrollLeft = Math.max(0, px - (mv.viewW - HEADER_W) * 0.15);
  });

  // ---- ruler ----------------------------------------------------------------------------------

  function scrub(e: PointerEvent) {
    if (e.button !== 0) return;
    const el = e.currentTarget as HTMLElement;
    const at = (ev: PointerEvent) => Math.max(0, Math.round(mv.samples(ev.clientX - el.getBoundingClientRect().left)));
    el.setPointerCapture(e.pointerId);
    if (e.shiftKey) return markRange(e, el, at);
    void preview.seek(at(e));
    const move = (ev: PointerEvent) => {
      mv.playhead = at(ev);
    };
    const up = (ev: PointerEvent) => {
      el.removeEventListener("pointermove", move);
      el.removeEventListener("pointerup", up);
      el.removeEventListener("pointercancel", up);
      if (preview.playing) void preview.seek(at(ev));
    };
    el.addEventListener("pointermove", move);
    el.addEventListener("pointerup", up);
    el.addEventListener("pointercancel", up);
  }

  /** Shift-drag on the ruler marks a stretch for "Export audio"; a Shift-click without a drag clears it. */
  function markRange(e: PointerEvent, el: HTMLElement, at: (ev: PointerEvent) => number) {
    const a = at(e);
    const set = (ev: PointerEvent) => {
      const b = at(ev);
      mv.marked = Math.abs(b - a) >= SR / 10 ? { from: Math.min(a, b), to: Math.max(a, b) } : null;
    };
    const up = () => {
      el.removeEventListener("pointermove", set);
      el.removeEventListener("pointerup", up);
      el.removeEventListener("pointercancel", up);
    };
    set(e);
    el.addEventListener("pointermove", set);
    el.addEventListener("pointerup", up);
    el.addEventListener("pointercancel", up);
  }

  // ---- bar and menus --------------------------------------------------------------------------

  /** Timeline sample under a pointer on a track element. */
  function sampleAt(e: MouseEvent): number {
    const track = (e.currentTarget as HTMLElement).getBoundingClientRect();
    return Math.max(0, Math.round(mv.samples(e.clientX - track.left)));
  }

  function bgDown(e: PointerEvent) {
    if (e.button !== 0 || e.target !== e.currentTarget) return;
    if (!(e.shiftKey || e.ctrlKey || e.metaKey)) mixes.clearSelection();
  }

  /** The "Add tool" items for `clip`, placed at `at`. */
  function toolItems(clip: Clip | undefined, at: number): MenuItem[] {
    return toolChoices(toolPrefs.showTrans).map((t) => ({ label: t.label, onclick: () => addProcessTo(t.type, clip, at, t.vocal) }));
  }

  function trackMenu(e: MouseEvent) {
    e.preventDefault();
    const at = sampleAt(e);
    const items: MenuItem[] = [];
    items.push({ label: "Paste at playhead", disabled: !mixes.clipboard, onclick: pasteAtPlayhead });
    items.push({ label: "Move playhead here", onclick: () => void preview.seek(at) });
    menu = { x: e.clientX, y: e.clientY, items };
  }

  function clipMenu(e: MouseEvent, clip: Clip) {
    const { prev, next } = neighbours(doc, clip.id);
    const n = mixes.selected.length;
    const at = Math.min(Math.max(mv.playhead, clip.visibleStart), clip.visibleEnd - E.MIN_LENGTH);
    const items: MenuItem[] = [
      ...toolItems(clip, at).map((i) => ({ ...i, label: `Add ${i.label} here` })),
      { label: n > 1 ? `Duplicate ${n} items` : "Duplicate", onclick: duplicateSelection },
      { label: "Copy", onclick: () => mixes.copy() },
      { label: "Cut", onclick: () => mixes.cut() },
      { label: "Beat-match to previous", disabled: !prev, onclick: () => void beatmatch(clip.id) },
      { label: "Auto transition to next", disabled: !next, onclick: () => next && void autoTransition(clip.id, next.id) },
      { label: n > 1 ? `Delete ${n} items` : "Delete", danger: true, onclick: ondelete },
    ];
    menu = { x: Math.min(e.clientX, innerWidth - 230), y: Math.max(0, Math.min(e.clientY, innerHeight - items.length * 30 - 20)), items };
  }

  function processMenu(e: MouseEvent, _clip: Clip, p: ProcessDoc) {
    const items: MenuItem[] = [
      { label: p.bypassed ? "Enable" : "Mute", onclick: () => mixes.updateProcess(_clip.id, p.id, p.bypassed ? "Enable" : "Mute", (q) => ({ ...q, bypassed: !q.bypassed })) },
      { label: "Duplicate", onclick: duplicateSelection },
      { label: "Copy", onclick: () => mixes.copy() },
      { label: "Delete", danger: true, onclick: ondelete },
    ];
    menu = { x: e.clientX, y: e.clientY, items };
  }

  function transitionMenu(e: MouseEvent, t: TransitionDoc) {
    const items: MenuItem[] = [
      { label: t.bypassed ? "Enable" : "Mute", onclick: () => mixes.edit("Mute crossfade", (d) => E.updateTransition(d, t.id, (x) => ({ ...x, bypassed: !x.bypassed }))) },
      { label: "Delete", danger: true, onclick: ondelete },
    ];
    menu = { x: e.clientX, y: e.clientY, items };
  }

  /** The "+ Tool" button of the strip: a tool for the selected clip (or the one under the playhead). */
  function toolButton(e: MouseEvent) {
    e.stopPropagation(); // the menu closes on any click that reaches the window
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const clip = targetClip();
    menu = { x: r.left, y: r.bottom + 2, items: clip ? toolItems(clip, mv.playhead) : [{ label: "Add a track first", disabled: true, onclick: () => {} }] };
  }

  // ---- dropping tracks from the picker --------------------------------------------------------

  // Pointer drag from the picker (state/trackDrag.svelte.ts; HTML5 drag and drop does not reach the
  // page in the Windows app). The pointer's x is the time; the deck is chosen by the store.
  const dropSample = (clientX: number, left: number) => {
    const at = Math.max(0, Math.round(mv.samples(laneOffset(clientX, left))));
    const targets = E.snapTargets(doc, mixes.snap, [], [at - SR, at + SR], mixes.sectionLookup);
    return E.snap(at, targets, mixes.threshold);
  };
  const barDrop: DropOptions = {
    scope: "mix",
    accept: () => true,
    ondrop: (ids, _hit, p) => {
      if (ids.length === 0) return;
      void addTracks(ids, dropSample(p.x, p.node.getBoundingClientRect().left));
    },
  };

  /** Where a track dragged over the bar would start (a marker line), while a "mix" drag runs. */
  let dropAt = $state<number | null>(null);
  $effect(() => {
    if (!(trackDrag.active && trackDrag.scope === "mix") || !barEl) {
      dropAt = null;
      return;
    }
    const el = barEl;
    const move = (e: PointerEvent) => {
      const r = el.getBoundingClientRect();
      const inside = e.clientX >= r.left && e.clientX <= r.right && e.clientY >= r.top && e.clientY <= r.bottom;
      dropAt = inside ? dropSample(e.clientX, r.left) : null;
    };
    window.addEventListener("pointermove", move, true);
    return () => {
      window.removeEventListener("pointermove", move, true);
      dropAt = null;
    };
  });

  // ---- envelope overlay -----------------------------------------------------------------------

  function gainChange(c: Clip, nodes: EnvNode[]) {
    mixes.setEnvelope(c.id, { kind: "gain" }, nodes, { live: true });
  }
  function keyChange(c: Clip, nodes: EnvNode[]) {
    mixes.setEnvelope(c.id, { kind: "key" }, envToKey(nodes, c.key), { live: true });
  }
  function tempoChange(c: Clip, nodes: EnvNode[]) {
    mixes.edit("Change tempo", (d) => E.setTempoCurve(d, c.id, nodes.map((n) => ({ pos: n.pos, speed: n.value / c.bpm }))), { group: `bpm:${c.id}` });
  }
  const tempoNodes = (c: Clip): EnvNode[] => c.tempo.map((n) => ({ pos: n.pos, value: c.bpm * n.speed }));
  const overlayName = { gain: "Gain", tempo: "Tempo", key: "Key" } as const;
</script>

<div class="timeline" bind:this={root}>
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="scroller" bind:this={scroller} {onscroll}>
    <div class="inner" style:width="{HEADER_W + contentW}px">
      <!-- ruler -->
      <div class="row ruler" style:height="{RULER_H}px">
        <div class="hd corner">
          <span class="muted">{bpm.toFixed(2)} BPM</span>
        </div>
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div class="track" style:width="{contentW}px" onpointerdown={scrub} title="Click or drag to place the playhead. Shift-drag marks a stretch to export (Shift-click clears it).">
          {#each ticks.beats as s (s)}<i class="beat" style:left="{s * mv.pps}px"></i>{/each}
          {#each ticks.bars as b (b.bar)}
            <i class="bar" style:left="{b.sec * mv.pps}px"></i>
            {#if b.label}<span class="barno" style:left="{b.sec * mv.pps + 3}px">{b.bar}</span>{/if}
          {/each}
          {#each ticks.time as t (t.sec)}
            <i class="tick" style:left="{t.sec * mv.pps}px"></i>
            <span class="timeno" style:left="{t.sec * mv.pps + 3}px">{t.label}</span>
          {/each}
        </div>
      </div>

      <!-- the mix bar -->
      <div class="row" style:height="{BAR_H}px">
        <div class="hd">
          <b>Mix</b>
          <span class="muted">{doc.clips.length} track{doc.clips.length === 1 ? "" : "s"}, {doc.transitions.length} crossfade{doc.transitions.length === 1 ? "" : "s"}</span>
          <ul class="legend" aria-label="Clip colours">
            {#each doc.clips.slice(0, 8) as c (c.id)}
              <li style:--c={colourOf(c.id)} title={clipTitle(c)}>{clipTitle(c)}</li>
            {/each}
            {#if doc.clips.length > 8}<li class="more">and {doc.clips.length - 8} more</li>{/if}
          </ul>
        </div>
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div
          class="track mixbar"
          bind:this={barEl}
          style:width="{contentW}px"
          onpointerdown={bgDown}
          oncontextmenu={trackMenu}
          use:trackDropTarget={barDrop}
        >
          <div class="lane" style:top="{PAD}px" style:height="{LANE_H}px">
            {#each visibleClips as clip (clip.id)}
              <ClipBlock {clip} colour={colourOf(clip.id)} height={LANE_H} oncontext={clipMenu} />
            {/each}
            {#each visibleTransitions as t (t.id)}
              <FadeBlock {t} height={LANE_H} fromColour={colourOf(t.between?.from)} toColour={colourOf(t.between?.to)} oncontext={transitionMenu} />
            {/each}
            {#each gaps as g (g.from.id + g.to.id)}
              {#if inView(g.start, g.end)}
                <div class="ghost" style:left="{mv.x(g.start)}px" style:width="{Math.max(30, mv.x(g.end - g.start))}px" style:height="{LANE_H}px">
                  <div class="ghostbtns">
                    <button
                      title="Add a crossfade over the overlap of “{clipTitle(g.from)}” and “{clipTitle(g.to)}”"
                      onclick={() => mixes.addTransition(g.from.id, g.to.id, g.start, g.end - g.start)}>+ crossfade</button
                    >
                    <button title="Build a transition here with the recipe and length set in the Auto transition settings (select a crossfade or a clip to change them)" onclick={() => void autoTransition(g.from.id, g.to.id)}>Auto</button>
                  </div>
                </div>
              {/if}
            {/each}

            <!-- the envelope overlay: every clip's line, editable on the selected clip only -->
            {#if mv.overlay !== "none"}
              {#each visibleClips as c (c.id)}
                {@const w = Math.max(2, mv.x(c.visibleEnd - c.visibleStart))}
                {@const edit = mixes.selected.includes(c.id)}
                <div class="envclip" class:edit style:left="{mv.x(c.visibleStart)}px" style:width="{w}px" style:top="{ENV_TOP}px" style:height="{LANE_H - ENV_TOP}px" title={edit ? "" : `${overlayName[mv.overlay]} of ${clipTitle(c)}: select the clip to edit it`}>
                  {#if mv.overlay === "gain"}
                    <EnvelopeSvg
                      nodes={c.gain}
                      d0={c.trimStart}
                      d1={c.length - c.trimEnd}
                      min={-12}
                      max={6}
                      width={w}
                      height={LANE_H - ENV_TOP}
                      colour={edit ? "#fff" : colourOf(c.id)}
                      guides={[0]}
                      unit="dB"
                      label="Gain of {clipTitle(c)}"
                      editable={edit}
                      snapValue={(v) => Math.round(v * 10) / 10}
                      onchange={(n) => gainChange(c, n)}
                      ondone={() => mixes.endGesture()}
                    />
                  {:else if mv.overlay === "key"}
                    <EnvelopeSvg
                      nodes={keyToEnv(c.key)}
                      d0={c.trimStart}
                      d1={c.length - c.trimEnd}
                      min={-12}
                      max={12}
                      width={w}
                      height={LANE_H - ENV_TOP}
                      colour={edit ? "#fff" : colourOf(c.id)}
                      guides={[0]}
                      unit="st"
                      label="Key of {clipTitle(c)}"
                      editable={edit}
                      snapValue={Math.round}
                      onchange={(n) => keyChange(c, n)}
                      ondone={() => mixes.endGesture()}
                    />
                  {:else}
                    <EnvelopeSvg
                      nodes={tempoNodes(c)}
                      d0={c.trimStart}
                      d1={c.length - c.trimEnd}
                      min={c.bpm * 0.5}
                      max={c.bpm * 1.5}
                      width={w}
                      height={LANE_H - ENV_TOP}
                      colour={edit ? "#fff" : colourOf(c.id)}
                      guides={[c.bpm]}
                      unit="BPM"
                      label="Tempo of {clipTitle(c)}"
                      editable={edit}
                      snapValue={(v) => Math.round(v * 100) / 100}
                      onchange={(n) => tempoChange(c, n)}
                      ondone={() => mixes.endGesture()}
                    />
                  {/if}
                </div>
              {/each}
            {/if}

            {#if dropAt !== null}<div class="dropline" style:left="{mv.x(dropAt)}px" aria-hidden="true"><span>{mmss(dropAt / SR)}</span></div>{/if}
            {#if doc.clips.length === 0}
              <div class="hint">Drag tracks here from the table below, or select one and press “Add at playhead”.</div>
            {/if}
          </div>
        </div>
      </div>

      <!-- the tool strip, attached to the bar -->
      <div class="row fxrow" style:height="{Math.max(40, fxH)}px">
        <div class="hd">
          <span class="hrow"><b>Tools</b></span>
          <button class="mini" onclick={toolButton} title="Add a tool, loop, reverse or ignore to the selected clip (or the one under the playhead)">+ Tool ▾</button>
        </div>
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div class="track" style:width="{contentW}px" onpointerdown={bgDown} oncontextmenu={trackMenu}>
          {#each fx.items as it (it.p.id)}
            {#if inView(it.start, it.end)}
              <ToolBlock clip={it.clip} process={it.p} top={4 + it.row * (FX_ROW + 2)} height={FX_ROW} conflict={conflicts.has(it.p.id)} accent={colourOf(it.clip.id)} oncontext={processMenu} />
            {/if}
          {/each}
          {#if fx.items.length === 0}<div class="hint" style:top="12px">Right-click a clip, or use + Tool, to add Reverb, Echo, Vocal, EQ, a loop and more. They appear here under the clip they belong to.</div>{/if}
        </div>
      </div>

      <!-- playhead -->
      <!-- the clip keeps the line from showing over the sticky lane headers -->
      <div class="phwrap" style:clip-path="inset(0 0 0 {mv.scrollLeft + HEADER_W}px)">
        {#if mv.marked}
          <div class="xrange marked" style:left="{HEADER_W + mv.x(mv.marked.from)}px" style:width="{Math.max(2, mv.x(mv.marked.to - mv.marked.from))}px" title="Marked for export: {mmss(mv.marked.from / SR)} to {mmss(mv.marked.to / SR)}"></div>
        {/if}
        {#if mv.rangeBand}
          <div class="xrange" style:left="{HEADER_W + mv.x(mv.rangeBand.from)}px" style:width="{Math.max(2, mv.x(mv.rangeBand.to - mv.rangeBand.from))}px" title="The part of the mix the export will contain"></div>
        {/if}
        <div class="playhead" style:left="{HEADER_W + mv.x(mv.playhead)}px"><span>{mmss(mv.playhead / SR)}</span></div>
      </div>
    </div>
  </div>
</div>

{#if menu}
  <ContextMenu x={menu.x} y={menu.y} items={menu.items} onclose={() => (menu = null)} />
{/if}

<style>
  .timeline {
    user-select: none;
    position: relative;
    min-height: 0;
    min-width: 0;
    height: 100%;
    background: var(--deep);
  }
  .scroller {
    position: absolute;
    inset: 0;
    overflow: auto;
  }
  .inner {
    position: relative;
    min-height: 100%;
  }
  .row {
    display: flex;
    border-bottom: 1px solid var(--line);
    box-sizing: border-box;
  }
  .hd {
    position: sticky;
    left: 0;
    z-index: 8;
    flex: 0 0 132px;
    width: 132px;
    box-sizing: border-box;
    padding: 6px 8px;
    background: var(--panel);
    border-right: 1px solid var(--line);
    font-size: 12px;
    display: flex;
    flex-direction: column;
    gap: 3px;
    overflow: hidden;
  }
  .hrow {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 6px;
  }
  .hd b {
    color: var(--text);
    font-weight: 600;
  }
  .legend {
    list-style: none;
    margin: 4px 0 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
    overflow: hidden;
  }
  .legend li {
    font-size: 11px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    color: var(--text);
  }
  .legend li::before {
    content: "";
    display: inline-block;
    width: 8px;
    height: 8px;
    border-radius: 2px;
    margin-right: 5px;
    background: var(--c);
  }
  .legend li.more {
    color: var(--muted);
  }
  .legend li.more::before {
    display: none;
  }
  .track {
    position: relative;
    flex: 0 0 auto;
    height: 100%;
  }
  .track:global([data-drop-over]) {
    box-shadow: inset 0 0 0 2px var(--accent);
    background: color-mix(in srgb, var(--accent) 10%, transparent);
  }
  .mixbar {
    background: repeating-linear-gradient(0deg, transparent 0 47px, rgba(255, 255, 255, 0.03) 47px 48px);
  }
  .lane {
    position: absolute;
    left: 0;
    right: 0;
  }
  .ruler {
    position: sticky;
    top: 0;
    z-index: 6;
    background: var(--deep);
  }
  .ruler .hd {
    z-index: 9;
    justify-content: center;
  }
  .ruler .track {
    cursor: text;
    overflow: hidden;
  }
  .ruler i {
    position: absolute;
    bottom: 0;
    width: 1px;
    display: block;
  }
  .ruler .tick {
    top: 0;
    height: 15px;
    background: var(--line-hi);
  }
  .ruler .bar {
    height: 10px;
    background: var(--muted);
    opacity: 0.8;
  }
  .ruler .beat {
    height: 5px;
    background: var(--muted);
    opacity: 0.5;
  }
  .ruler span {
    position: absolute;
    font-size: 10px;
    pointer-events: none;
    white-space: nowrap;
  }
  .timeno {
    top: 1px;
    color: var(--text);
  }
  .barno {
    bottom: 12px;
    color: var(--muted);
  }
  .phwrap {
    position: absolute;
    inset: 0;
    pointer-events: none;
    z-index: 7;
  }
  .xrange {
    position: absolute;
    top: 0;
    bottom: 0;
    background: var(--accent-soft);
    border-left: 1px solid var(--accent);
    border-right: 1px solid var(--accent);
    box-sizing: border-box;
    pointer-events: none;
  }
  .xrange.marked {
    border-left-style: dashed;
    border-right-style: dashed;
    opacity: 0.7;
  }
  .playhead {
    position: absolute;
    top: 0;
    bottom: 0;
    width: 2px;
    margin-left: -1px;
    background: var(--green);
    box-shadow: 0 0 6px var(--green);
    pointer-events: none;
  }
  .playhead span {
    position: absolute;
    top: 20px;
    left: 4px;
    font-size: 10px;
    color: #000;
    background: var(--green);
    border-radius: 2px;
    padding: 0 3px;
    white-space: nowrap;
  }
  .hint {
    position: absolute;
    left: 14px;
    top: 22px;
    color: var(--muted);
    font-size: 12px;
    pointer-events: none;
    white-space: nowrap;
  }
  .ghost {
    position: absolute;
    top: 0;
    box-sizing: border-box;
    border: 1px dashed var(--line-hi);
    border-radius: 4px;
    background: rgba(0, 0, 0, 0.3);
    display: flex;
    align-items: center;
    justify-content: center;
    overflow: hidden;
    pointer-events: none;
    z-index: 4;
  }
  .ghostbtns {
    display: flex;
    flex-direction: column;
    gap: 6px;
    pointer-events: auto;
    max-width: 100%;
  }
  .ghost button,
  .mini {
    padding: 2px 8px;
    font-size: 11px;
    white-space: nowrap;
  }
  .envclip {
    position: absolute;
    overflow: hidden;
    pointer-events: none;
    z-index: 5;
  }
  .envclip.edit {
    background: rgba(0, 0, 0, 0.3);
  }
  .envclip.edit :global(svg) {
    pointer-events: auto;
  }
  .dropline {
    position: absolute;
    top: -4px;
    bottom: -4px;
    width: 2px;
    margin-left: -1px;
    background: var(--accent);
    box-shadow: 0 0 6px var(--accent);
    pointer-events: none;
    z-index: 6;
  }
  .dropline span {
    position: absolute;
    top: 2px;
    left: 4px;
    font-size: 10px;
    color: #000;
    background: var(--accent);
    border-radius: 2px;
    padding: 0 3px;
    white-space: nowrap;
  }
</style>
