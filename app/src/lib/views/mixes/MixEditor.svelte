<script lang="ts">
  // The open mix: toolbar, library picker, timeline, inspector and transport, plus the keyboard.
  import { tick } from "svelte";
  import * as E from "$lib/state/mixedit";
  import { mixes } from "$lib/state/mixes.svelte";
  import { toast } from "$lib/state/toast.svelte";
  import Button from "$lib/ui/Button.svelte";
  import { describe, duplicateSelection, errText, pasteAtPlayhead } from "./actions";
  import { confirm } from "./confirm.svelte";
  import { cancelExport, closeMix, exportAudio, newMix, saveMix } from "./flow";
  import Inspector from "./Inspector.svelte";
  import { PPS_MAX, PPS_MIN, SR, clock, ppsFromSlider, sliderFromPps } from "./layout";
  import { preview } from "./preview.svelte";
  import Timeline from "./Timeline.svelte";
  import TrackPicker from "./TrackPicker.svelte";
  import { mv } from "./view.svelte";

  const doc = $derived(mixes.doc!);
  let saving = $state(false);

  // fit a mix to the window when it is opened
  let fittedFor = -1;
  $effect(() => {
    const id = mixes.doc?.id ?? -1;
    if (id !== fittedFor) {
      fittedFor = id;
      const len = mixes.doc?.length ?? 0;
      // wait for the timeline to measure itself, then fit
      void tick()
        .then(() => new Promise((r) => requestAnimationFrame(r)))
        .then(() => (len > 0 ? mv.fit(len) : (mv.pps = 24)));
    }
  });

  async function save() {
    if (saving || !mixes.doc) return;
    saving = true;
    try {
      await saveMix();
    } catch (e) {
      toast.show(`Not saved: ${errText(e)}`, "bad", 10000);
    } finally {
      saving = false;
    }
  }

  /** Deleting always asks first and names each thing it takes (a clip with its tools and curves,
   * a tool, a crossfade). Ctrl+Z brings it back until the mix is saved and closed. */
  function requestDelete() {
    const d = mixes.doc;
    if (!d || mixes.selected.length === 0) return;
    const ids = [...mixes.selected];
    const lines = ids.map((id) => {
      const f = E.find(d, id);
      let line = describe(d, id);
      if (f?.kind === "clip" && f.clip.processes.length > 0) line += `, with its ${f.clip.processes.length} tool${f.clip.processes.length === 1 ? "" : "s"}`;
      return line;
    });
    // crossfades that join a deleted clip go with it
    for (const t of d.transitions) {
      if (t.between && (ids.includes(t.between.from) || ids.includes(t.between.to)) && !ids.includes(t.id)) lines.push(describe(d, t.id) + " (joins a deleted clip)");
    }
    confirm.ask({
      title: ids.length === 1 ? "Delete from the mix" : `Delete ${ids.length} items from the mix`,
      message: "These will be removed from the mix (Ctrl+Z brings them back until you save and close):",
      items: lines,
      confirmLabel: "Delete",
      danger: true,
      run: () => mixes.deleteSelection(),
    });
  }

  function editable(t: EventTarget | null): boolean {
    const el = t as HTMLElement | null;
    if (!el || !el.tagName) return false;
    return el.isContentEditable || ["INPUT", "TEXTAREA", "SELECT"].includes(el.tagName) && !(el as HTMLInputElement).type?.match(/^(range|checkbox|radio|button)$/);
  }

  function onkey(e: KeyboardEvent) {
    if (!mixes.doc) return;
    const mod = e.ctrlKey || e.metaKey;
    const k = e.key.toLowerCase();
    if (mod && k === "s") {
      e.preventDefault();
      void save();
      return;
    }
    if (editable(e.target)) return;
    // a dialog is open, or a button / menu item has the focus and wants Space and Enter itself
    if (document.querySelector('[role="dialog"]')) return;
    if (k === " " && (e.target as HTMLElement | null)?.closest?.("button, a, summary, [role=menuitem]")) return;
    if (mod && k === "z") {
      e.preventDefault();
      if (e.shiftKey) mixes.redo();
      else mixes.undo();
    } else if (mod && k === "y") {
      e.preventDefault();
      mixes.redo();
    } else if (mod && k === "c") {
      e.preventDefault();
      mixes.copy();
    } else if (mod && k === "x") {
      e.preventDefault();
      mixes.cut();
    } else if (mod && k === "v") {
      e.preventDefault();
      pasteAtPlayhead();
    } else if (mod && k === "d") {
      e.preventDefault();
      duplicateSelection();
    } else if (mod && k === "a") {
      e.preventDefault();
      mixes.selectAllClips();
    } else if (!mod && (k === "delete" || k === "backspace")) {
      e.preventDefault();
      requestDelete();
    } else if (!mod && k === " ") {
      e.preventDefault();
      void preview.toggle();
    } else if (!mod && k === "escape") {
      mixes.clearSelection();
    } else if (!mod && k === "home") {
      void preview.seek(0);
      mv.scrollLeft = 0;
    } else if (!mod && k === "end") {
      toEnd();
    } else if (!mod && (k === "=" || k === "+")) {
      mv.setZoom(mv.pps * 1.25);
    } else if (!mod && (k === "-" || k === "_")) {
      mv.setZoom(mv.pps / 1.25);
    }
  }

  function onbeforeunload(e: BeforeUnloadEvent) {
    if (mixes.dirty) e.preventDefault();
  }

  /** Moves the playhead to the end of the mix and scrolls it into view. */
  function toEnd() {
    const d = mixes.doc;
    if (!d) return;
    void preview.seek(d.length);
    const px = mv.x(d.length);
    if (px < mv.scrollLeft || px > mv.scrollLeft + mv.viewW - 80) mv.scrollLeft = Math.max(0, px - mv.viewW + 120);
  }

  async function playPause() {
    if (!preview.playing && preview.status !== "rendering") {
      // bring the playhead into view before starting
      const px = mv.x(mv.playhead);
      if (px < mv.scrollLeft || px > mv.scrollLeft + mv.viewW - 200) mv.scrollLeft = Math.max(0, px - 80);
    }
    await preview.toggle();
  }

  const zoomValue = $derived(sliderFromPps(mv.pps));
  const busyLabel = $derived(preview.status === "rendering" ? "Rendering preview…" : "");
</script>

<svelte:window onkeydown={onkey} {onbeforeunload} />

<div class="editor">
  <div class="bar">
    <Button variant="ghost" onclick={closeMix} title="Back to the list of mixes">← Mixes</Button>
    <h2 title={doc.name}>{doc.name}{#if mixes.dirty}<span class="dirty" title="Unsaved changes"> ●</span>{/if}</h2>
    <span class="chip">mix · {doc.clips.length} track{doc.clips.length === 1 ? "" : "s"} · {clock(doc.lengthSec)}</span>
    <Button onclick={newMix}>New</Button>
    <Button variant="primary" disabled={!mixes.dirty || saving || mixes.busy} onclick={save} title={mixes.problems.length ? `Cannot save yet: ${mixes.problems[0]}` : "Save (Ctrl+S)"}>Save</Button>
    <span class="sep"></span>
    <Button disabled={mixes.undoDepth === 0} onclick={() => mixes.undo()} title={mixes.undoLabel ? `Undo ${mixes.undoLabel} (Ctrl+Z)` : "Nothing to undo"} aria-label="Undo">↶</Button>
    <Button disabled={mixes.redoDepth === 0} onclick={() => mixes.redo()} title={mixes.redoLabel ? `Redo ${mixes.redoLabel} (Ctrl+Y)` : "Nothing to redo"} aria-label="Redo">↷</Button>
    <span class="sep"></span>
    <label class="ctl">Snap
      <select bind:value={mixes.snap}>
        <option value="beat">Beat</option>
        <option value="bar">Bar</option>
        <option value="phrase">Phrase (8 bars)</option>
        <option value="off">Off</option>
      </select>
    </label>
    <label class="ctl">Zoom
      <input
        type="range"
        min="0"
        max="100"
        step="0.5"
        value={zoomValue}
        oninput={(e) => mv.setZoom(ppsFromSlider(e.currentTarget.valueAsNumber))}
        aria-label="Zoom"
        title="{mv.pps.toFixed(1)} px per second ({PPS_MIN}–{PPS_MAX}). Ctrl+wheel zooms at the pointer."
      />
    </label>
    <Button small onclick={() => mv.fit(doc.length)} title="Show the whole mix">Fit</Button>
    <Button small active={mv.pickerOpen} onclick={() => (mv.pickerOpen = !mv.pickerOpen)}>Tracks</Button>
    <span class="grow"></span>
    {#if mixes.problems.length > 0}
      <span class="problem" title={mixes.problems.join("\n")}>⚠ {mixes.problems.length} problem{mixes.problems.length === 1 ? "" : "s"} blocking save</span>
    {/if}
    <Button disabled={mv.exporting || doc.clips.length === 0} onclick={() => void exportAudio()} title="Render the mix to an audio file">
      {mv.exporting ? (mv.exportPct > 0 ? `Exporting ${Math.round(mv.exportPct * 100)}%` : "Exporting…") : "Export audio"}
    </Button>
    {#if mv.exporting}
      <Button onclick={() => void cancelExport()} title="Stop rendering; no file is kept">Cancel export</Button>
    {/if}
  </div>
  {#if mv.exporting}<div class="progress" role="progressbar" aria-label="Exporting audio"><i></i></div>{/if}

  <div class="split">
    {#if mv.pickerOpen}<TrackPicker />{/if}
    <div class="tl"><Timeline ondelete={requestDelete} /></div>
    <Inspector ondelete={requestDelete} />
  </div>

  <div class="transport">
    <Button aria-label="Back to start" onclick={() => { void preview.seek(0); mv.scrollLeft = 0; }}>⏮</Button>
    <Button variant="primary" aria-label={preview.playing ? "Pause" : "Play"} onclick={playPause} disabled={doc.clips.length === 0}>
      {preview.playing ? "❚❚ Pause" : preview.status === "rendering" ? "Rendering…" : "▶ Play"}
    </Button>
    <Button aria-label="To the end" onclick={toEnd}>⏭</Button>
    <span class="time" aria-live="off">{clock(mv.playhead / SR)}</span>
    <span class="muted">/ {clock(doc.lengthSec)}</span>
    {#if busyLabel}<span class="muted">{busyLabel}</span>{/if}
    {#if preview.stale && (preview.playing || preview.status === "paused")}
      <span class="stale" title="The preview was rendered before your last edit. Press play again to hear the change.">preview is out of date</span>
    {/if}
    {#if preview.notice}<span class="notice" role="status">{preview.notice}</span>{/if}
    <label class="ctl" title="Preview volume only; it is not saved in the mix">Master
      <input type="range" min="0" max="1" step="0.01" bind:value={preview.volume} aria-label="Preview volume" />
    </label>
    <label class="ctl follow"><input type="checkbox" bind:checked={mv.follow} /> follow playhead</label>
    <span class="legend">drag clips · click a block to edit it · Space plays from the playhead</span>
  </div>
</div>

<style>
  .editor {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
    min-width: 0;
  }
  .bar {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 8px;
    padding: 8px 12px;
    border-bottom: 1px solid var(--line);
  }
  h2 {
    margin: 0;
    font-size: 16px;
    max-width: 280px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .dirty {
    color: var(--accent);
  }
  .chip {
    color: var(--muted);
    font-size: 12px;
  }
  .sep {
    width: 1px;
    height: 20px;
    background: var(--line);
  }
  .grow {
    flex: 1;
  }
  .ctl {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    color: var(--muted);
    font-size: 12px;
  }
  .ctl input[type="range"] {
    width: 100px;
  }
  .problem {
    color: var(--bad);
    font-size: 12px;
    cursor: help;
  }
  .progress {
    height: 3px;
    background: var(--line);
    overflow: hidden;
    position: relative;
  }
  .progress i {
    position: absolute;
    inset: 0 auto 0 0;
    width: 30%;
    background: var(--accent-ramp);
    animation: slide 1.1s linear infinite;
  }
  @keyframes slide {
    from {
      left: -30%;
    }
    to {
      left: 100%;
    }
  }
  .split {
    flex: 1;
    min-height: 0;
    display: flex;
  }
  .tl {
    flex: 1;
    min-width: 0;
    min-height: 0;
  }
  .transport {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 7px 12px;
    background: var(--deep);
    border-top: 1px solid var(--line);
    flex-wrap: wrap;
  }
  .time {
    font: 600 18px ui-monospace, Consolas, monospace;
    color: var(--green);
    min-width: 84px;
    text-align: right;
  }
  .muted {
    color: var(--muted);
  }
  .stale {
    color: var(--warn);
    font-size: 12px;
  }
  .notice {
    color: var(--bad-text);
    background: var(--bad-bg);
    border-radius: 4px;
    padding: 2px 8px;
    font-size: 12px;
  }
  .follow {
    margin-left: 8px;
  }
  .legend {
    margin-left: auto;
    color: var(--muted);
    font-size: 12px;
  }
</style>
