<script lang="ts">
  // The timeline: ruler, two deck lanes with clips, the crossfade lane, one effects lane per
  // deck and the tempo / key / gain envelope lanes, with the playhead over all of them.
  //
  // Only what is near the viewport is rendered (clips, blocks, ruler marks, waveform canvases),
  // so a long mix with many clips stays as cheap as a short one.
  import type { Clip, EnvNode, ProcessDoc, TransitionDoc } from "$lib/api/mixes";
  import * as E from "$lib/state/mixedit";
  import { mixes } from "$lib/state/mixes.svelte";
  import { vocal } from "$lib/state/vocal.svelte";
  import ContextMenu, { type MenuItem } from "$lib/ui/ContextMenu.svelte";
  import { addProcessAt, addTracks, autoTransition, beatmatch, clipTitle, duplicateSelection, neighbours, pasteAtPlayhead } from "./actions";
  import ClipBlock from "./ClipBlock.svelte";
  import EnvelopeSvg from "./EnvelopeSvg.svelte";
  import { HEADER_W, PPS_MAX, PPS_MIN, SR, clamp, envToKey, keyToEnv, packRows, rulerTicks, mmss } from "./layout";
  import { preview } from "./preview.svelte";
  import ToolBlock from "./ToolBlock.svelte";
  import TransitionBlock from "./TransitionBlock.svelte";
  import { mv } from "./view.svelte";

  let { ondelete }: { ondelete: () => void } = $props();

  const RULER_H = 40;
  const DECK_H = 64;
  const CLIP_H = 52;
  const XF_H = 46;
  const FX_ROW = 24;
  const ENV_H = 60;

  const doc = $derived(mixes.doc!);
  let scroller = $state<HTMLDivElement | null>(null);
  let root = $state<HTMLDivElement | null>(null);
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

  const decks = $derived([0, 1].map((d) => doc.clips.filter((c) => c.channel === d)));
  const visibleDecks = $derived(decks.map((cs) => cs.filter((c) => inView(c.visibleStart, c.visibleEnd))));

  /** Tool lane of each deck: every process of the deck's clips, packed into rows. */
  const fx = $derived(
    decks.map((cs) => {
      const items: { clip: Clip; p: ProcessDoc; start: number; end: number }[] = [];
      for (const clip of cs) for (const p of clip.processes) items.push({ clip, p, start: clip.start + p.start, end: clip.start + p.start + p.length });
      const { rows, count } = packRows(items);
      return { items: items.map((it, i) => ({ ...it, row: rows[i] })), count };
    }),
  );
  const conflicts = $derived.by(() => {
    const out = new Set<string>();
    for (const c of doc.clips) {
      for (const p of c.processes) if (p.kind === "tool") for (const id of E.toolConflicts(c, p)) out.add(id);
    }
    return out;
  });

  const visibleTransitions = $derived(doc.transitions.filter((t) => inView(t.start, t.start + t.length)));

  /** Pairs of clips that overlap on different decks with no crossfade yet. */
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
    const ro = new ResizeObserver(() => (mv.viewW = s.clientWidth));
    ro.observe(s);
    mv.viewW = s.clientWidth;
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

  // ---- lanes ----------------------------------------------------------------------------------

  function laneAt(clientY: number): number | null {
    if (!root) return null;
    let best: number | null = null;
    let dist = Infinity;
    root.querySelectorAll<HTMLElement>("[data-deck]").forEach((el) => {
      const r = el.getBoundingClientRect();
      const d = clientY >= r.top && clientY <= r.bottom ? 0 : Math.min(Math.abs(clientY - r.top), Math.abs(clientY - r.bottom));
      if (d < dist) {
        dist = d;
        best = Number(el.dataset.deck);
      }
    });
    return best;
  }

  /** Timeline sample under a pointer on a lane's track element. */
  function sampleAt(e: MouseEvent | DragEvent): number {
    const track = (e.currentTarget as HTMLElement).getBoundingClientRect();
    return Math.max(0, Math.round(mv.samples(e.clientX - track.left)));
  }

  function bgDown(e: PointerEvent) {
    if (e.button !== 0 || e.target !== e.currentTarget) return;
    if (!(e.shiftKey || e.ctrlKey || e.metaKey)) mixes.clearSelection();
  }

  function fxMenu(deck: number, at: number): MenuItem[] {
    const add = (type: string, label: string): MenuItem => ({ label, onclick: () => addProcessAt(type, deck, at) });
    return [
      ...E.toolTypesFor(vocal.mode).map((t) => add(t.name, t.label)),
      add("loop", "Loop"),
      add("reverse", "Reverse"),
      add("ignore", "Ignore"),
    ];
  }

  function trackMenu(e: MouseEvent, deck: number | null, kind: "deck" | "fx" | "other") {
    e.preventDefault();
    const at = sampleAt(e);
    const items: MenuItem[] = [];
    if (kind === "fx" && deck !== null) items.push(...fxMenu(deck, at).map((i) => ({ ...i, label: `Add ${i.label}` })));
    items.push({ label: "Paste at playhead", disabled: !mixes.clipboard, onclick: pasteAtPlayhead });
    items.push({ label: "Move playhead here", onclick: () => void preview.seek(at) });
    menu = { x: e.clientX, y: e.clientY, items };
  }

  function clipMenu(e: MouseEvent, clip: Clip) {
    const { prev, next } = neighbours(doc, clip.id);
    const n = mixes.selected.length;
    const items: MenuItem[] = [
      { label: n > 1 ? `Duplicate ${n} items` : "Duplicate", onclick: duplicateSelection },
      { label: "Copy", onclick: () => mixes.copy() },
      { label: "Cut", onclick: () => mixes.cut() },
      { label: "Beat-match to previous", disabled: !prev, onclick: () => void beatmatch(clip.id) },
      { label: "Auto transition to next", disabled: !next, onclick: () => next && void autoTransition(clip.id, next.id) },
      { label: clip.channel === 0 ? "Move to channel 2" : "Move to channel 1", disabled: n > 1, onclick: () => mixes.setChannel(clip.id, clip.channel === 0 ? 1 : 0) },
      { label: n > 1 ? `Delete ${n} items` : "Delete", danger: true, onclick: ondelete },
    ];
    menu = { x: e.clientX, y: e.clientY, items };
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

  // ---- dropping tracks from the picker --------------------------------------------------------

  const MIME = "application/x-pm-tracks";
  function dragover(e: DragEvent) {
    if (e.dataTransfer?.types.includes(MIME)) {
      e.preventDefault();
      e.dataTransfer.dropEffect = "copy";
    }
  }
  function drop(e: DragEvent, deck: number) {
    const raw = e.dataTransfer?.getData(MIME);
    if (!raw) return;
    e.preventDefault();
    let ids: number[] = [];
    try {
      ids = JSON.parse(raw);
    } catch {
      return;
    }
    let at = sampleAt(e);
    const targets = E.snapTargets(doc, mixes.snap, [], [at - SR, at + SR]);
    at = E.snap(at, targets, mixes.threshold);
    void addTracks(ids, at, deck);
  }

  // ---- envelope lane helpers ------------------------------------------------------------------

  function gainChange(c: Clip, nodes: EnvNode[]) {
    mixes.setEnvelope(c.id, { kind: "gain" }, nodes, { live: true });
  }
  function keyChange(c: Clip, nodes: EnvNode[]) {
    mixes.setEnvelope(c.id, { kind: "key" }, envToKey(nodes, c.key), { live: true });
  }
  function tempoLine(c: Clip, bpmValue: number) {
    mixes.edit("Change tempo", (d) => E.setClipBpm(d, c.id, bpmValue), { group: `bpm:${c.id}` });
  }
  const tempoNodes = (c: Clip): EnvNode[] => c.tempo.map((n) => ({ pos: n.pos, value: c.bpm * n.speed }));

  const envClips = $derived(doc.clips.filter((c) => inView(c.visibleStart, c.visibleEnd)));
  const laneIds = ["tempo", "key", "gain"] as const;
  const laneTitle = { tempo: "Tempo", key: "Key", gain: "Gain" } as const;
  const laneSub = { tempo: "BPM", key: "semitones", gain: "dB" } as const;
</script>

<div class="timeline" bind:this={root}>
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="scroller" bind:this={scroller} {onscroll}>
    <div class="inner" style:width="{HEADER_W + contentW}px">
      <!-- ruler -->
      <div class="row ruler" style:height="{RULER_H}px">
        <div class="hd corner">
          <span class="muted">{bpm.toFixed(1)} BPM</span>
        </div>
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div class="track" style:width="{contentW}px" onpointerdown={scrub} title="Click or drag to place the playhead">
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

      <!-- deck lanes -->
      {#each [0, 1] as deck (deck)}
        <div class="row" style:height="{DECK_H}px">
          <div class="hd">
            <b>Channel {deck + 1}</b>
            <span class="muted">deck {deck ? "B" : "A"} · {decks[deck].length} clip{decks[deck].length === 1 ? "" : "s"}</span>
          </div>
          <!-- svelte-ignore a11y_no_static_element_interactions -->
          <div
            class="track"
            data-deck={deck}
            style:width="{contentW}px"
            onpointerdown={bgDown}
            oncontextmenu={(e) => trackMenu(e, deck, "deck")}
            ondragover={dragover}
            ondrop={(e) => drop(e, deck)}
          >
            {#each visibleDecks[deck] as clip (clip.id)}
              <ClipBlock {clip} height={CLIP_H} {laneAt} oncontext={clipMenu} />
            {/each}
            {#if doc.clips.length === 0 && deck === 0}
              <div class="hint">Drag tracks here from the Tracks panel, or press “Add at playhead”.</div>
            {/if}
          </div>
        </div>
      {/each}

      <!-- transitions -->
      <div class="row" style:height="{XF_H + 4}px">
        <div class="hd">
          <b>Transitions</b>
          <span class="muted">{doc.transitions.length} crossfade{doc.transitions.length === 1 ? "" : "s"}</span>
        </div>
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div class="track" style:width="{contentW}px" onpointerdown={bgDown} oncontextmenu={(e) => trackMenu(e, null, "other")}>
          {#each visibleTransitions as t (t.id)}
            <TransitionBlock {t} height={XF_H - 4} oncontext={transitionMenu} />
          {/each}
          {#each gaps as g (g.from.id + g.to.id)}
            {#if inView(g.start, g.end)}
              <div class="ghost" style:left="{mv.x(g.start)}px" style:width="{Math.max(26, mv.x(g.end - g.start))}px" style:height="{XF_H - 4}px">
                <button
                  title="Add a crossfade over the overlap of “{clipTitle(g.from)}” and “{clipTitle(g.to)}”"
                  onclick={() => mixes.addTransition(g.from.id, g.to.id, g.start, g.end - g.start)}>+ X-fade</button
                >
                <button title="Crossfade with a bass swap, as suggested from the beat grids" onclick={() => void autoTransition(g.from.id, g.to.id)}>Auto</button>
              </div>
            {/if}
          {/each}
        </div>
      </div>

      <!-- effects lanes, one per deck -->
      {#each [0, 1] as deck (deck)}
        {@const h = fx[deck].count * (FX_ROW + 2) + 8}
        <div class="row" style:height="{Math.max(54, h)}px">
          <div class="hd">
            <span class="hrow"><b>Effects {deck + 1}</b></span>
            <button
              class="mini"
              onclick={(e) => {
                e.stopPropagation(); // the menu closes on any click that reaches the window
                const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
                menu = { x: r.left, y: r.bottom + 2, items: fxMenu(deck, mv.playhead) };
              }}>+ Tool ▾</button
            >
          </div>
          <!-- svelte-ignore a11y_no_static_element_interactions -->
          <div class="track" style:width="{contentW}px" onpointerdown={bgDown} oncontextmenu={(e) => trackMenu(e, deck, "fx")}>
            {#each fx[deck].items as it (it.p.id)}
              {#if inView(it.start, it.end)}
                <ToolBlock clip={it.clip} process={it.p} top={4 + it.row * (FX_ROW + 2)} height={FX_ROW} conflict={conflicts.has(it.p.id)} oncontext={processMenu} />
              {/if}
            {/each}
          </div>
        </div>
      {/each}

      <!-- envelope lanes -->
      {#each laneIds as lane (lane)}
        {#if mv.lanes[lane]}
          <div class="row" style:height="{ENV_H}px">
            <div class="hd">
              <span class="hrow"><b>{laneTitle[lane]}</b><button class="mini" onclick={() => (mv.lanes[lane] = false)} title="Hide this lane">Hide</button></span>
              <span class="muted">{laneSub[lane]}</span>
            </div>
            <!-- svelte-ignore a11y_no_static_element_interactions -->
            <div class="track env" style:width="{contentW}px" onpointerdown={bgDown}>
              {#each envClips as c (c.id)}
                {@const w = Math.max(2, mv.x(c.visibleEnd - c.visibleStart))}
                <div class="envclip" class:sel={mixes.selected.includes(c.id)} style:left="{mv.x(c.visibleStart)}px" style:width="{w}px">
                  {#if lane === "gain"}
                    <EnvelopeSvg
                      nodes={c.gain}
                      d0={c.trimStart}
                      d1={c.length - c.trimEnd}
                      min={-12}
                      max={6}
                      width={w}
                      height={ENV_H}
                      colour="var(--green)"
                      guides={[0]}
                      unit="dB"
                      label="Gain of {clipTitle(c)}"
                      snapValue={(v) => Math.round(v * 10) / 10}
                      onchange={(n) => gainChange(c, n)}
                      ondone={() => mixes.endGesture()}
                    />
                  {:else if lane === "key"}
                    <EnvelopeSvg
                      nodes={keyToEnv(c.key)}
                      d0={c.trimStart}
                      d1={c.length - c.trimEnd}
                      min={-12}
                      max={12}
                      width={w}
                      height={ENV_H}
                      colour="var(--accent)"
                      guides={[0]}
                      unit="st"
                      label="Key of {clipTitle(c)}"
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
                      height={ENV_H}
                      colour="var(--l2a)"
                      guides={[c.bpm]}
                      unit="BPM"
                      label="Tempo of {clipTitle(c)}"
                      editable={c.tempo.length === 1}
                      snapValue={(v) => Math.round(v * 100) / 100}
                      lineDrag={(v) => tempoLine(c, v)}
                      ondone={() => mixes.endGesture()}
                    />
                  {/if}
                </div>
              {/each}
            </div>
          </div>
        {/if}
      {/each}

      {#if !(mv.lanes.tempo && mv.lanes.key && mv.lanes.gain)}
        <div class="row lanebar" style:height="28px">
          <div class="hd"><span class="muted">Envelopes</span></div>
          <div class="track" style:width="{contentW}px">
            <span class="lanebtns">
              {#each laneIds as lane (lane)}
                {#if !mv.lanes[lane]}<button class="mini" onclick={() => (mv.lanes[lane] = true)}>+ {laneTitle[lane]} lane</button>{/if}
              {/each}
            </span>
          </div>
        </div>
      {/if}

      <!-- playhead -->
      <!-- the clip keeps the line from showing over the sticky lane headers -->
      <div class="phwrap" style:clip-path="inset(0 0 0 {mv.scrollLeft + HEADER_W}px)">
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
  .track {
    position: relative;
    flex: 0 0 auto;
    height: 100%;
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
    top: 4px;
    box-sizing: border-box;
    border: 1px dashed var(--line-hi);
    border-radius: 4px;
    display: flex;
    gap: 4px;
    align-items: center;
    justify-content: center;
    overflow: hidden;
  }
  .ghost button,
  .mini {
    padding: 1px 6px;
    font-size: 11px;
    white-space: nowrap;
  }
  .env {
    background: repeating-linear-gradient(0deg, transparent 0 29px, var(--line) 29px 30px);
  }
  .envclip {
    position: absolute;
    top: 0;
    bottom: 0;
    overflow: hidden;
    background: rgba(255, 255, 255, 0.025);
    border-left: 1px solid var(--line);
    border-right: 1px solid var(--line);
  }
  .envclip.sel {
    background: var(--accent-soft);
  }
  .lanebar .track {
    display: flex;
    align-items: center;
  }
  .lanebtns {
    position: sticky;
    left: 140px;
    display: flex;
    gap: 6px;
    padding-left: 8px;
  }
</style>
