<script lang="ts">
  // One clip on a channel lane: colour waveform, title, drag to move (snapped; between decks by
  // dragging up or down), drag an edge to trim. Shift / Ctrl-click adds to the selection.
  import type { Clip } from "$lib/api/mixes";
  import * as E from "$lib/state/mixedit";
  import { effectiveBpm } from "$lib/state/mixedit";
  import { mixes } from "$lib/state/mixes.svelte";
  import { HEADER_W, SR } from "./layout";
  import ClipWave from "./ClipWave.svelte";
  import { mv } from "./view.svelte";

  let {
    clip,
    height,
    laneAt,
    oncontext,
  }: {
    clip: Clip;
    height: number;
    /** Deck (0 or 1) whose lane is under a viewport y, or null. */
    laneAt: (clientY: number) => number | null;
    oncontext: (e: MouseEvent, clip: Clip) => void;
  } = $props();

  const EDGE = 7;
  const left = $derived(mv.x(clip.visibleStart));
  const width = $derived(Math.max(2, mv.x(clip.visibleEnd - clip.visibleStart)));
  const selected = $derived(mixes.selected.includes(clip.id));
  const broken = $derived(!clip.track || !clip.track.available || clip.trackId < 0);

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
    const z = zone(e);
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
        const snapped = snapOn
          ? E.snapClipStart(doc0, clip.id, primary, { mode: mixes.snap, threshold: mixes.threshold, exclude: ids })
          : primary;
        const d = snapped - orig.get(clip.id)!;
        const deck = ids.length === 1 ? laneAt(ev.clientY) : null;
        mixes.edit(
          "Move clips",
          (doc) =>
            ids.reduce(
              (acc, id) =>
                E.moveClip(acc, id, orig.get(id)! + d, { mode: "off", threshold: 0, channel: id === clip.id && deck !== null ? deck : undefined }),
              doc,
            ),
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
        let pos = base + mv.samples(ev.clientX - startX);
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
  class:edge={hover !== "move"}
  role="button"
  tabindex="0"
  aria-label="Clip {title}"
  aria-pressed={selected}
  data-clip={clip.id}
  style:left="{left}px"
  style:width="{width}px"
  style:height="{height}px"
  onpointerdown={down}
  onpointermove={(e) => (hover = zone(e))}
  onkeydown={key}
  oncontextmenu={ctx}
>
  <ClipWave {clip} pps={mv.pps} from={win.from} to={win.to} {height} />
  <span class="grip left"></span>
  <span class="grip right"></span>
  <span class="title" style:max-width="{Math.max(0, width - 8)}px">{title}<em>{tempoText}</em></span>
  {#if broken}<span class="warn" title={clip.track?.problem ?? "track unavailable"}>{clip.track?.problem ?? "track unavailable"}</span>{/if}
  {#if clip.trimStart > SR * 0.05}<span class="cue" title="Cue point: audio before it is hidden"></span>{/if}
</div>

<style>
  .clip {
    position: absolute;
    top: 6px;
    box-sizing: border-box;
    border: 1px solid var(--line-hi);
    border-radius: 5px;
    background: var(--row);
    overflow: hidden;
    cursor: grab;
    outline: none;
  }
  .clip:active {
    cursor: grabbing;
  }
  .clip.edge {
    cursor: ew-resize;
  }
  .clip.selected {
    border-color: var(--accent);
    box-shadow: 0 0 0 1px var(--accent);
  }
  .clip:focus-visible {
    border-color: var(--text);
  }
  .clip.bypass {
    opacity: 0.5;
  }
  .clip.broken {
    background: repeating-linear-gradient(135deg, var(--bad-bg) 0 8px, var(--deep) 8px 16px);
    border-color: var(--bad);
  }
  .title {
    position: absolute;
    left: 6px;
    top: 2px;
    font-size: 11px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    text-shadow: 0 0 3px #000, 0 0 3px #000;
    pointer-events: none;
    z-index: 2;
  }
  .title em {
    margin-left: 8px;
    font-style: normal;
    color: var(--muted);
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
  .grip {
    position: absolute;
    top: 0;
    bottom: 0;
    width: 3px;
    background: transparent;
    z-index: 3;
    pointer-events: none;
  }
  .grip.left {
    left: 0;
  }
  .grip.right {
    right: 0;
  }
  .selected .grip {
    background: var(--accent);
  }
  .cue {
    position: absolute;
    left: 0;
    top: 0;
    border-style: solid;
    border-width: 8px 8px 0 0;
    border-color: var(--green) transparent transparent transparent;
    pointer-events: none;
    z-index: 3;
  }
</style>
