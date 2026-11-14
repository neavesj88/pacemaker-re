<script lang="ts">
  // The open mix: toolbar, library picker, timeline, inspector and transport, plus the keyboard.
  import { onMount, tick, untrack } from "svelte";
  import { claimSpace } from "$lib/player/spaceClaim";
  import * as E from "$lib/state/mixedit";
  import { mixExport } from "$lib/state/mixexport.svelte";
  import { mixes } from "$lib/state/mixes.svelte";
  import { sections } from "$lib/state/sections.svelte";
  import { tracklist } from "$lib/state/tracklist.svelte";
  import { toast } from "$lib/state/toast.svelte";
  import Button from "$lib/ui/Button.svelte";
  import { describe, clipTitle, duplicateSelection, errText, pasteAtPlayhead } from "./actions";
  import BpmField from "./BpmField.svelte";
  import { confirm } from "./confirm.svelte";
  import { cancelExport, closeMix, exportAudio, newMix, reloadChangedMix, saveMix, saveMixAs } from "./flow";
  import Inspector from "./Inspector.svelte";
  import { PPS_MAX, PPS_MIN, SR, clock, ppsFromSlider, sliderFromPps } from "./layout";
  import { preview } from "./preview.svelte";
  import MixPicker from "./MixPicker.svelte";
  import Timeline from "./Timeline.svelte";
  import { stretchText } from "$lib/state/tempoMatch";
  import { mv, type EnvOverlay } from "./view.svelte";
  import { setlist } from "../setlist/setlist.svelte";

  const doc = $derived(mixes.doc!);
  // Space plays the mix here; the player bar's own Space shortcut stands aside while a mix is open
  onMount(() => claimSpace(() => !!mixes.doc));
  let saving = $state(false);
  tracklist.ensureLoaded();
  mixExport.ensureLoaded();

  function openExport() {
    mixExport.startFor(mixes.doc?.id);
    mixExport.open = true;
  }

  // The live engine plays the open mix as it stands: opened (decoding starts) when a mix is opened,
  // (preview.svelte.ts then sends every edit, including the Vocal marks of tools, which only the app knows).
  let openedId = -1;
  $effect(() => {
    const d = mixes.doc;
    if (!d || d.id === openedId) return;
    openedId = d.id;
    untrack(() => void preview.open(d));
  });
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
    // a row, clip or control already acted on this key (one key, one action)
    if (e.defaultPrevented) return;
    if (mod && k === "s") {
      e.preventDefault();
      if (e.shiftKey) saveMixAs();
      else void save();
      return;
    }
    if (editable(e.target)) return;
    // the library picker is a table with its own keys (Space auditions a track in the player bar,
    // Delete and Ctrl+A are not for the clips): only undo and redo reach the mix from there
    if ((e.target as Element | null)?.closest?.(".picker") && !(mod && (k === "z" || k === "y"))) return;
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
  const busyLabel = $derived(preview.status === "rendering" ? "Rendering the export preview…" : preview.loading ? `Reading the tracks: ${preview.load.loaded} of ${preview.load.total}` : "");

  /** The mix tempo the toolbar shows: the reference tempo, else the first clip's. */
  const mixTempo = $derived(doc.tempoBpm ?? (doc.clips[0] ? E.effectiveBpm(doc.clips[0], doc.clips[0].trimStart) : null));
  function setMixTempo(v: number) {
    mixes.setTempo(v);
  }
  /** Plays every clip at the mix tempo (or double or half of it), keeping any that would need a big stretch. */
  function matchAll() {
    if (!mixTempo) return;
    const r = mixes.syncAll(mixTempo);
    const bits: string[] = [];
    if (r.applied > 0) bits.push(`${r.applied} track${r.applied === 1 ? "" : "s"} matched to ${mixTempo.toFixed(2)} BPM.`);
    if (r.already > 0 && r.applied > 0) bits.push(`${r.already} already played at it.`);
    if (r.applied === 0 && r.kept.length === 0 && r.ramps.length === 0) bits.push("Every track already plays at the mix tempo.");
    if (r.ramps.length > 0) bits.push(`Left alone because they have a tempo ramp (use Sync on the clip to rescale it): ${r.ramps.map((c) => clipTitle(c)).join("; ")}.`);
    if (r.kept.length > 0) bits.push(`Left at their own tempo, because matching would stretch them too much: ${r.kept.map((k) => `${clipTitle(k.clip)} (${stretchText(k.choice.stretchPct)})`).join("; ")}.`);
    toast.show(bits.join(" "), r.kept.length > 0 || r.ramps.length > 0 ? "info" : "ok", 9000);
  }
  const OVERLAYS: { id: EnvOverlay; label: string; tip: string }[] = [
    { id: "gain", label: "Gain", tip: "Draw each clip's gain curve over the bar; select a clip to edit its curve (click adds a node, right-click removes it)" },
    { id: "tempo", label: "Tempo", tip: "Draw each clip's tempo curve over the bar; select a clip and click to add a node for a tempo ramp" },
    { id: "key", label: "Key", tip: "Draw each clip's key curve over the bar; select a clip to edit it" },
  ];
</script>

<svelte:window onkeydown={onkey} {onbeforeunload} />

<div class="editor">
  <div class="bar">
    <Button variant="ghost" onclick={closeMix} title="Back to the list of mixes">← Mixes</Button>
    <h2 title={doc.name}>{doc.name}{#if mixes.dirty}<span class="dirty" title="Unsaved changes"> ●</span>{/if}</h2>
    <span class="chip">mix · {doc.clips.length} track{doc.clips.length === 1 ? "" : "s"} · {clock(doc.lengthSec)}</span>
    <Button onclick={newMix}>New</Button>
    <Button variant="primary" disabled={!mixes.dirty || saving || mixes.busy} onclick={save} title={mixes.problems.length ? `Cannot save yet: ${mixes.problems[0]}` : "Save (Ctrl+S)"}>Save</Button>
    <Button
      disabled={saving || mixes.busy}
      onclick={saveMixAs}
      title={mixes.problems.length ? `Cannot save yet: ${mixes.problems[0]}` : "Save the mix as a new mix under another name; this one keeps its last saved content (Ctrl+Shift+S)"}>Save as…</Button
    >
    <span class="sep"></span>
    <Button disabled={mixes.undoDepth === 0} onclick={() => mixes.undo()} title={mixes.undoLabel ? `Undo ${mixes.undoLabel} (Ctrl+Z)` : "Nothing to undo"} aria-label="Undo">↶</Button>
    <Button disabled={mixes.redoDepth === 0} onclick={() => mixes.redo()} title={mixes.redoLabel ? `Redo ${mixes.redoLabel} (Ctrl+Y)` : "Nothing to redo"} aria-label="Redo">↷</Button>
    <span class="sep"></span>
    <label class="ctl">Snap
      <select bind:value={mixes.snap} title="Section snaps to where detected sections begin (intro, build, drop, breakdown, outro; Tracks, Sections panel). A clip whose track has none snaps to its phrase lines instead.">
        <option value="beat">Beat</option>
        <option value="bar">Bar</option>
        <option value="phrase">Phrase (8 bars)</option>
        <option value="section">Section</option>
        <option value="off">Off</option>
      </select>
    </label>
    <label class="ctl follow" title={sections.showMarkers ? "Draw the detected sections (intro, build, drop, breakdown, outro) on the clips. Clips of tracks without sections show nothing." : "Turned off in the Sections panel's settings (show markers on waveforms)"}>
      <input type="checkbox" checked={mv.sectionMarkers && sections.showMarkers} disabled={!sections.showMarkers} onchange={(e) => mv.setSectionMarkers(e.currentTarget.checked)} /> sections
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
    <span class="sep"></span>
    <span class="ctl" title="The tempo the mix is matched to: new tracks and the Match buttons play at this, or at double or half of it, whichever stretches the track least. Not stored in the mix file.">
      Mix tempo
      <BpmField value={mixTempo} onset={setMixTempo} label="Mix tempo" disabled={doc.clips.length === 0} />
    </span>
    <Button small disabled={!mixTempo} onclick={matchAll} title="Play every track at the mix tempo (or double or half of it); a track that would need more stretch than the setting allows keeps its own tempo">Match all</Button>
    <span class="ctl">Curves
      <span class="segs">
        {#each OVERLAYS as o (o.id)}
          <Button small active={mv.overlay === o.id} onclick={() => (mv.overlay = mv.overlay === o.id ? "none" : o.id)} title={o.tip}>{o.label}</Button>
        {/each}
      </span>
    </span>
    <Button small active={mv.pickerOpen} onclick={() => (mv.pickerOpen = !mv.pickerOpen)} title="Show or hide the track table under the mix">Tracks</Button>
    <div class="acts">
      {#if mixes.problems.length > 0}
        <span class="problem" title={mixes.problems.join("\n")}>⚠ {mixes.problems.length} problem{mixes.problems.length === 1 ? "" : "s"} blocking save</span>
      {/if}
      <Button disabled={doc.clips.length === 0} onclick={() => (tracklist.open = true)} title="Start times of the tracks as text, CSV, YouTube chapters or a CUE sheet. Its settings also say whether Export audio writes a .cue beside the audio.">Track list</Button>
      <Button disabled={doc.clips.length === 0} onclick={() => setlist.openMix(doc.id)} title="A printable setlist of this mix">Setlist…</Button>
      <Button
        disabled={mv.exporting || doc.clips.length === 0}
        onclick={openExport}
        title="Render the mix, or part of it, to an audio file. Choose the part and a target loudness first."
      >
        {mv.exporting
          ? `${mv.exportPhase === "measuring" ? "Measuring" : "Exporting"}${mv.exportPct > 0 ? ` ${Math.round(mv.exportPct * 100)}%` : "…"}`
          : "Export audio…"}
      </Button>
      {#if mv.exporting}
        <Button onclick={() => void cancelExport()} title="Stop rendering; no file is kept">Cancel export</Button>
      {/if}
    </div>
  </div>
  {#if mv.exporting}<div class="progress" role="progressbar" aria-label="Exporting audio"><i></i></div>{/if}
  {#if mixes.libraryChange}
    <div class="stale" role="alert">
      {#if mixes.libraryChange === "gone"}
        <span>“{doc.name}” is no longer in the library: a restore or sync removed it. Your edits are still here; use Save as to keep them as a new mix.</span>
      {:else}
        <span>“{doc.name}” was changed in the library by a restore or sync after you opened it. Saving now would overwrite that version with the one on screen.</span>
        <Button onclick={reloadChangedMix} title="Read the mix from the library and drop the edits made here">Reload (drop my edits)</Button>
        <Button onclick={() => mixes.keepEdits()} title="Allow Save to replace the library's version with this one">Keep my edits</Button>
      {/if}
    </div>
  {/if}

  <div class="body">
    <div class="left">
      <div class="tl"><Timeline ondelete={requestDelete} /></div>

      <div class="transport">
        <Button aria-label="Back to start" onclick={() => { void preview.seek(0); mv.scrollLeft = 0; }}>⏮</Button>
        <Button variant="primary" aria-label={preview.playing ? "Pause" : "Play"} onclick={playPause} disabled={doc.clips.length === 0}>
          {preview.playing ? "❚❚ Pause" : preview.status === "rendering" ? "Rendering…" : "▶ Play"}
        </Button>
        <Button aria-label="To the end" onclick={toEnd}>⏭</Button>
        <span class="time" aria-live="off">{clock(mv.playhead / SR)}</span>
        <span class="muted">/ {clock(doc.lengthSec)}</span>
        {#if busyLabel}<span class="muted">{busyLabel}</span>{/if}
        {#if preview.notice}<span class="notice" role="status">{preview.notice}</span>{/if}
        {#if mixExport.previewGains()}
          <span class="level" title="The preview plays at the level your last loudness analysis found for the export settings.">export level</span>
        {:else if mixExport.previewBehind}
          <span class="stale" title="Loudness settings are on but the last analysis is about a different mix or settings. Analyse again in Export audio to hear the export level.">not at export level</span>
        {/if}
        <label class="ctl" title="Preview volume only; it is not saved in the mix">Master
          <input type="range" min="0" max="1" step="0.01" bind:value={preview.volume} aria-label="Preview volume" />
        </label>
        <label class="ctl follow"><input type="checkbox" bind:checked={mv.follow} /> follow playhead</label>
        <span class="legend">plays live as you edit · drag clips · click a block to edit it · Space plays from the playhead</span>
      </div>
      {#if mv.pickerOpen}<MixPicker />{/if}
    </div>
    <Inspector ondelete={requestDelete} />
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
  /* the right-hand actions stay together: when the toolbar is too narrow they move to a second row as a group */
  .acts {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-left: auto;
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
  .stale {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
    padding: 6px 12px;
    background: var(--panel);
    border-bottom: 1px solid var(--cue);
    color: var(--text);
    font-size: 12px;
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
  .body {
    flex: 1;
    min-height: 0;
    display: flex;
  }
  .left {
    flex: 1;
    min-width: 0;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }
  .segs {
    display: inline-flex;
    gap: 2px;
  }
  .tl {
    flex: 1 1 0;
    min-width: 0;
    min-height: 250px;
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
  .level {
    color: var(--green);
    font-size: 12px;
    border: 1px solid var(--line-hi);
    border-radius: 4px;
    padding: 1px 6px;
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
