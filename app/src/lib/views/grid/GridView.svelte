<script lang="ts">
  import { onMount, untrack } from "svelte";
  import { nav } from "$lib/state/nav.svelte";
  import { playback } from "$lib/state/playback.svelte";
  import { selection } from "$lib/state/selection.svelte";
  import { session } from "$lib/state/session.svelte";
  import Button from "$lib/ui/Button.svelte";
  import Modal from "$lib/ui/Modal.svelte";
  import { toast } from "$lib/state/toast.svelte";
  import { gridMixesUsing } from "$lib/api/grid";
  import { editor, tempoMap } from "./editor.svelte";
  import { firstDownbeatSecs, fmtTime, lowerBound } from "./gridMath";
  import { claimSpace } from "$lib/player/spaceClaim";
  import { Metronome } from "./metronome";
  import SectionList from "../analysis/SectionList.svelte";
  import Overview from "./Overview.svelte";
  import TempoPanel from "./TempoPanel.svelte";
  import TempoStrip from "./TempoStrip.svelte";
  import WaveCanvas from "./WaveCanvas.svelte";
  import { vtempo } from "./vtempo.svelte";
  import { dragLimits, sectionLines } from "./vtempoLogic";

  // ---- which track ----
  const tracks = $derived(session.library?.tracks ?? []);
  const meta = $derived(tracks.find((t) => t.track_id === editor.trackId));
  const label = (t: { artist: string; title: string; location: string }) =>
    t.artist && t.title ? `${t.artist} - ${t.title}` : t.title || t.location;

  let pendingOpen = $state<number | null>(null);
  let confirmSave = $state(false);
  /** The cue unit is unverified on a device: saving a cue needs an explicit tick. */
  let cueOk = $state(false);
  /** Names of the mixes that use the open track, listed before a grid is saved. */
  let usedByMixes = $state<string[]>([]);
  let mixesOk = $state(false);
  /** Bumped when a track switch is refused so the picker is rebuilt showing the open track. */
  let pickerKey = $state(0);

  function open(id: number) {
    cursor = 0;
    viewStart = 0;
    follow = true;
    pxPerSec = 0; // the waveform refits when the new track's length arrives
    editor.open(id);
  }

  /** Switching away from unsaved edits asks first. */
  function request(id: number) {
    if (id === editor.trackId) return;
    if (editor.dirty) pendingOpen = id;
    else open(id);
  }

  onMount(() => {
    const want = nav.take<number>("grid.trackId") ?? nav.take<number>("trackId");
    if (typeof want === "number") request(want);
    else if (editor.trackId === null) {
      // nothing open yet: start from the highlighted or playing track
      const first = selection.trackIds[0] ?? playback.trackId;
      if (first !== null && first !== undefined) open(first);
    }
  });

  // a deep link while the view is already showing
  $effect(() => {
    const id = nav.params["grid.trackId"] ?? nav.params["trackId"];
    if (typeof id !== "number") return;
    untrack(() => {
      nav.take("grid.trackId");
      nav.take("trackId");
      request(id);
    });
  });

  // ---- view window ----
  let viewStart = $state(0);
  /** Scroll with the playhead while playing; any manual scroll, drag or zoom turns it off until
   *  the next play or seek. */
  let follow = $state(true);
  let pxPerSec = $state(0);
  let width = $state(0);
  let wave = $state<ReturnType<typeof WaveCanvas> | null>(null);
  const viewSpan = $derived(pxPerSec > 0 ? width / pxPerSec : 0);

  // ---- playhead: the transport's position when our track is loaded there, else a cursor ----
  let cursor = $state(0);
  let head = $state(0);
  let stamp = 0;
  let stampPos = 0;
  const loadedHere = $derived(editor.trackId !== null && playback.trackId === editor.trackId);

  // the player reports position in coarse steps: extrapolate between them for a smooth playhead
  $effect(() => {
    stampPos = playback.position;
    stamp = performance.now();
  });
  const livePos = () => (loadedHere && playback.playing ? stampPos + (performance.now() - stamp) / 1000 : loadedHere ? playback.position : cursor);

  $effect(() => {
    if (!(loadedHere && playback.playing)) {
      head = loadedHere ? playback.position : cursor;
      return;
    }
    let raf = 0;
    const loop = () => {
      head = Math.min(livePos(), editor.duration || Infinity);
      const span = viewSpan;
      if (follow && span > 0 && (head > viewStart + span * 0.92 || head < viewStart)) {
        viewStart = Math.max(0, Math.min(head - span * 0.08, Math.max(0, editor.duration - span)));
      }
      raf = requestAnimationFrame(loop);
    };
    // The first frame runs here, inside the effect. It writes `head` and reads it back (and
    // `viewStart`, `follow`, `viewSpan`), so tracked it made the effect depend on what it writes
    // and Svelte stopped it with effect_update_depth_exceeded as soon as the track played. Later
    // frames run from requestAnimationFrame, outside any effect.
    untrack(loop);
    return () => cancelAnimationFrame(raf);
  });

  function seek(secs: number) {
    cursor = secs;
    follow = true;
    if (viewSpan > 0 && (secs < viewStart || secs > viewStart + viewSpan)) wave?.centre(secs);
    if (loadedHere) playback.seek(secs);
  }

  function togglePlay() {
    const id = editor.trackId;
    if (id === null) return;
    if (!(loadedHere && playback.playing)) follow = true;
    if (!loadedHere) {
      playback.load(id, true);
      if (cursor > 0) playback.seek(cursor);
    } else playback.toggle();
  }

  // ---- metronome ----
  const metro = new Metronome();
  let click = $state(false);
  let latency = $state(0);
  const beatSecs = $derived(Float64Array.from(editor.doc?.beats ?? [], (b) => b.pos / editor.sampleRate));
  const fd = $derived(editor.doc?.first_downbeat ?? null);

  $effect(() => {
    metro.latencyMs = latency;
  });

  // ---- variable tempo: sections and drift follow the working grid ----
  // (declared in this order: a new track is forgotten before its sections are asked for)
  let toolsFor: number | null = null;
  $effect(() => {
    const id = editor.trackId;
    untrack(() => {
      if (id === toolsFor) return;
      toolsFor = id;
      vtempo.forget();
      if (id !== null && vtempo.settings.auto_check) void vtempo.checkAudio();
    });
  });
  $effect(() => {
    void editor.doc;
    void editor.splits;
    void vtempo.settings.section_tolerance_pct;
    void vtempo.settings.section_min_beats;
    untrack(() => void vtempo.refreshSections());
  });
  $effect(() => {
    void editor.doc;
    void vtempo.onsets;
    void vtempo.driftOptions;
    untrack(() => void vtempo.refreshDrift());
  });

  const beatFrames = $derived((editor.doc?.beats ?? []).map((b) => b.pos));
  const limitsFor = (i: number): [number, number] | null => {
    const [lo, hi] = dragLimits(beatFrames, i, editor.sampleRate, editor.loaded?.total_frames || null);
    return lo <= hi ? [lo / editor.sampleRate, hi / editor.sampleRate] : null;
  };
  const stripSections = $derived(
    vtempo.sections.map((s) => ({ start: s.start_frame / editor.sampleRate, end: s.end_frame / editor.sampleRate, bpm: s.bpm, forced: s.forced })),
  );
  const stripSpans = $derived(
    (vtempo.currentDrift?.spans ?? []).map((s) => ({ start: s.start_frame / editor.sampleRate, end: s.end_frame / editor.sampleRate })),
  );

  $effect(() => {
    if (!click || !loadedHere || !playback.playing) return;
    metro.reset();
    const beats = beatSecs;
    const first = fd;
    const h = setInterval(() => metro.tick(livePos(), beats, first), 20);
    return () => clearInterval(h);
  });

  async function toggleClick() {
    if (!click) await metro.arm();
    click = !click;
  }

  onMount(() => () => metro.close());
  // Space plays this track here; the player bar's own Space shortcut stands aside while this is open
  onMount(() => claimSpace(() => editor.trackId !== null));

  // ---- editing helpers ----
  let bpmText = $state("");
  $effect(() => {
    bpmText = editor.bpm ? editor.bpm.toFixed(2) : "";
  });

  async function applyBpm() {
    const v = Number(bpmText.replace(",", "."));
    if (!Number.isFinite(v) || v <= 0) {
      toast.show("Enter a tempo in BPM, for example 128 or 127.5", "bad");
      bpmText = editor.bpm ? editor.bpm.toFixed(2) : "";
      return;
    }
    // the box shows the median tempo: re-applying it would flatten a variable-tempo grid for nothing
    if (Math.abs(v - editor.bpm) < 0.01) {
      bpmText = editor.bpm.toFixed(2);
      return;
    }
    await editor.setBpm(v);
  }

  const canEdit = $derived(!!editor.doc && !editor.busy);
  const downbeatSecs = $derived(firstDownbeatSecs(editor.doc, editor.sampleRate));
  const beatNo = $derived(beatSecs.length ? Math.max(0, lowerBound(beatSecs, head + 0.0005) - 1) + 1 : 0);
  const cueSecs = $derived(editor.cue === null ? null : editor.cue / editor.sampleRate);
  const strPath = $derived((editor.loaded?.location ?? "").replace(/\.[^./]+$/, ".str"));
  const signed = (n: number, digits = 1) => `${n >= 0 ? "+" : "-"}${Math.abs(n).toFixed(digits)}`;

  function onkey(e: KeyboardEvent) {
    const t = e.target as HTMLElement | null;
    if (t && /^(INPUT|SELECT|TEXTAREA)$/.test(t.tagName)) return;
    if (confirmSave || pendingOpen !== null) return;
    const mod = e.ctrlKey || e.metaKey;
    if (mod && e.key.toLowerCase() === "z") {
      e.preventDefault();
      if (e.shiftKey) editor.redo();
      else editor.undo();
    } else if (mod && e.key.toLowerCase() === "y") {
      e.preventDefault();
      editor.redo();
    } else if (!mod && !e.altKey && e.key.toLowerCase() === "t" && editor.trackId !== null) {
      // tap tempo from the keyboard; the tempo tools show the result
      if (!e.repeat) vtempo.tap();
      e.preventDefault();
    } else if (e.key === " " && editor.trackId !== null) {
      // also when a toolbar button has focus: keydown is consumed here and keyup is cancelled
      // below, so the focused button does not fire a second time
      e.preventDefault();
      togglePlay();
    }
  }

  function onkeyup(e: KeyboardEvent) {
    const t = e.target as HTMLElement | null;
    if (e.key === " " && t instanceof HTMLButtonElement && !confirmSave && pendingOpen === null) e.preventDefault();
  }

  const needsCueTick = $derived(editor.cueDirty && editor.cue !== null);

  const needsMixTick = $derived(editor.gridDirty && usedByMixes.length > 0);

  /** Opens the confirmation. A new grid changes the tempo the track has, but a mix clip keeps the
   *  tempo it had when the mix was made: the mixes that use the track are looked up and named
   *  first, and the save needs a tick. If the lookup fails the save is not offered blind. */
  async function askSave() {
    cueOk = false;
    mixesOk = false;
    usedByMixes = [];
    const lib = session.library;
    // the confirmation lists the tempo sections: make sure they describe the grid being saved
    await vtempo.refreshSections();
    if (editor.gridDirty && lib && editor.loaded) {
      try {
        usedByMixes = await gridMixesUsing(lib.root, editor.loaded.track_id);
      } catch (err) {
        toast.show(`Could not check which mixes use this track: ${String(err)}`, "bad", 8000);
        return;
      }
    }
    confirmSave = true;
  }

  async function doSave() {
    confirmSave = false;
    await editor.save();
  }
</script>

<svelte:window onkeydown={onkey} onkeyup={onkeyup} />

<div class="grid">
  {#if !session.library}
    <p class="empty">Open a library to edit beat grids.</p>
  {:else}
    <div class="bar">
      {#key pickerKey}
        <select
          aria-label="Track"
          value={editor.trackId ?? ""}
          onchange={(e) => request(Number(e.currentTarget.value))}
        >
          {#if editor.trackId === null}<option value="" disabled>Choose a track…</option>{/if}
          {#each tracks as t (t.track_id)}<option value={t.track_id}>{label(t)}</option>{/each}
        </select>
      {/key}
      {#if editor.loaded}
        <span class="chip" class:g={!editor.dirty && !!editor.doc} class:bad={!editor.doc}>
          {!editor.doc ? "no grid" : editor.dirty ? "unsaved changes" : "saved"}
        </span>
        {#if editor.doc}
          <span class="muted num">
            {editor.bpm.toFixed(2)} BPM{meta?.key ? ` · ${meta.key}` : ""} · first downbeat {downbeatSecs === null ? "not set" : fmtTime(downbeatSecs)}
          </span>
        {/if}
      {/if}
      <span class="spacer"></span>
      <Button small disabled={!editor.canUndo || editor.busy} onclick={() => editor.undo()} title="Undo (Ctrl+Z)">Undo</Button>
      <Button small disabled={!editor.canRedo || editor.busy} onclick={() => editor.redo()} title="Redo (Ctrl+Shift+Z)">Redo</Button>
      <Button variant="primary" small disabled={!editor.dirty || editor.busy} onclick={() => void askSave()}>Save…</Button>
    </div>

    {#if editor.loaded}
      <div class="tools">
        <div class="grp" role="group" aria-label="Nudge the whole grid">
          <span class="lab">Nudge</span>
          {#each [-20, -5, -1, 1, 5, 20] as ms (ms)}
            <Button small disabled={!canEdit} onclick={() => editor.nudgeMs(ms)} title="Move every beat {ms < 0 ? 'earlier' : 'later'} by {Math.abs(ms)} ms">
              {ms > 0 ? "+" : "−"}{Math.abs(ms)}
            </Button>
          {/each}
          <span class="lab">ms</span>
        </div>
        <div class="grp" role="group" aria-label="Beat lines">
          <Button small disabled={!canEdit} onclick={() => editor.shiftBeats(-1)} title="Move the beat lines one beat earlier">◀ beat</Button>
          <Button small disabled={!canEdit} onclick={() => editor.shiftBeats(1)} title="Move the beat lines one beat later">beat ▶</Button>
          <Button small disabled={!canEdit} onclick={() => editor.setDownbeatAt(head)} title="Make the beat nearest the playhead a bar line">Set downbeat</Button>
          <Button
            small
            active={vtempo.editBeats}
            disabled={!editor.doc}
            onclick={() => (vtempo.editBeats = !vtempo.editBeats)}
            title="Grab single beat lines on the waveform and drag them (Alt: with every later beat). Tempo tools are below the waveform."
          >Drag beats</Button>
        </div>
        <div class="grp" role="group" aria-label="Tempo">
          <Button small disabled={!canEdit} onclick={() => editor.halve()} title="Keep every second beat">½ BPM</Button>
          <Button small disabled={!canEdit} onclick={() => editor.double()} title="Add a beat between every two">×2 BPM</Button>
          <input
            class="bpm num"
            aria-label="BPM"
            inputmode="decimal"
            bind:value={bpmText}
            disabled={!canEdit}
            onkeydown={(e) => e.key === "Enter" && applyBpm()}
          />
          <Button small disabled={!canEdit} onclick={applyBpm} title="Rebuild a constant grid at this tempo from the first beat">Set BPM</Button>
        </div>
        <div class="grp" role="group" aria-label="Analysis and cue">
          <Button small disabled={editor.busy} onclick={() => editor.reanalyse()} title="Replace the grid with a fresh analysis of the audio (nothing is saved until you save)">Re-analyse</Button>
          <Button small disabled={editor.busy} onclick={() => editor.setCue(head)} title="Use the playhead as the device cue point">Set cue</Button>
          <Button small disabled={editor.busy || editor.cue === null} onclick={() => editor.setCue(null)}>Clear cue</Button>
        </div>
      </div>

      <div class="transport">
        <Button small onclick={togglePlay}>{loadedHere && playback.playing ? "❚❚ Pause" : "▶ Play"}</Button>
        <span class="num time">{fmtTime(head)}</span>
        <span class="muted num">beat {beatNo}</span>
        <span class="spacer"></span>
        <Button small active={click} onclick={toggleClick} title="Click on every beat while playing; bar lines click higher">Metronome</Button>
        {#if click}
          <label class="muted" title="Shift the click later (+) or earlier (−) to match your output device">
            latency <input class="lat num" type="number" step="5" bind:value={latency} /> ms
          </label>
        {/if}
        <Button small onclick={() => wave?.zoom(1 / 1.6)} aria-label="Zoom out">−</Button>
        <Button small onclick={() => wave?.zoom(1.6)} aria-label="Zoom in">+</Button>
        <Button small onclick={() => wave?.fit()}>Fit</Button>
        <Button small active={follow} onclick={() => (follow = !follow)} title="Scroll with the playhead while playing">Follow</Button>
      </div>

      {#if editor.error}<p class="err">{editor.error}</p>{/if}
      {#if editor.peaksError}
        <p class="err">
          Waveform unavailable: {editor.peaksError}
          <Button small onclick={() => editor.loadPeaks()}>Retry</Button>
        </p>
      {/if}
      {#if !editor.doc}
        <p class="warn">
          No usable beat grid ({editor.loaded.problem ?? "unknown"}). Use Re-analyse to build one from the audio, then Save.
        </p>
      {/if}

      <WaveCanvas
        bind:this={wave}
        bind:viewStart
        bind:pxPerSec
        bind:width
        peaks={editor.peaks}
        {beatSecs}
        firstDownbeat={fd}
        duration={editor.duration}
        cue={cueSecs}
        playhead={head}
        onseek={seek}
        onuser={() => (follow = false)}
        status={editor.peaksLoading ? "Loading waveform…" : ""}
        editable={vtempo.editBeats && canEdit}
        bind:selected={vtempo.selected}
        {limitsFor}
        onbeatmove={(i, secs, following) => vtempo.moveBeat(i, editor.frames(secs), following)}
        offsets={vtempo.currentDrift?.offsets_ms ?? null}
        biasMs={vtempo.currentDrift?.bias_ms ?? 0}
        thresholdMs={vtempo.settings.drift_threshold_ms}
        grabPx={vtempo.settings.grab_px}
      />
      <TempoStrip
        {beatSecs}
        sections={stripSections}
        offsets={vtempo.currentDrift?.offsets_ms ?? null}
        biasMs={vtempo.currentDrift?.bias_ms ?? 0}
        thresholdMs={vtempo.settings.drift_threshold_ms}
        spans={stripSpans}
        duration={editor.duration}
        playhead={head}
        {viewStart}
        {pxPerSec}
        {width}
        checking={vtempo.onsetsLoading}
        onseek={seek}
      />
      <Overview
        trackId={editor.trackId}
        peaks={editor.peaks}
        {beatSecs}
        firstDownbeat={fd}
        duration={editor.duration}
        cue={cueSecs}
        playhead={head}
        {viewStart}
        {viewSpan}
        oncentre={(s) => {
          follow = false;
          wave?.centre(s);
        }}
      />
      <SectionList
        trackId={editor.trackId}
        onseek={(s) => {
          follow = false;
          wave?.centre(s);
        }}
      />

      {#if editor.doc}<TempoPanel {head} onseek={seek} />{/if}

      <div class="cols">
        <div class="card">
          <h4>Grid</h4>
          <div class="field"><span class="lab">BPM</span><span class="num">{editor.doc ? editor.bpm.toFixed(2) : "-"}</span></div>
          <div class="field">
            <span class="lab">Offset</span>
            <span class="num">{signed(editor.offsetMs)} ms</span>
          </div>
          <div class="field"><span class="lab">Beats</span><span class="num">{editor.doc?.beats.length ?? 0}</span></div>
          <div class="field"><span class="lab">Tempo map</span><span>{tempoMap(editor.doc)}</span></div>
          <p class="note">Writes the same .str file the Pacemaker reads, so sync locks to these beats.</p>
        </div>
        <div class="card">
          <h4>Device cue</h4>
          <div class="field">
            <span class="lab">Cue point</span>
            <span class="num">{cueSecs === null ? "not set" : fmtTime(cueSecs)}{editor.cueDirty ? " (unsaved)" : ""}</span>
          </div>
          <p class="note">
            The Pacemaker holds one cue point per track. It is stored in the track's row with the grid, in source sample
            frames. The unit of a set cue is unverified against a device.
          </p>
        </div>
        <div class="card">
          <h4>Keys</h4>
          <div class="field"><span class="lab">Space</span><span>play / pause</span></div>
          <div class="field"><span class="lab">Ctrl+Z</span><span>undo (Ctrl+Shift+Z redo)</span></div>
          <div class="field"><span class="lab">Wheel</span><span>zoom; Shift+wheel or drag to scroll</span></div>
        </div>
      </div>
    {:else if editor.busy}
      <p class="empty">Loading…</p>
    {:else}
      <p class="empty">{editor.error || "Choose a track above to check and correct its beat grid."}</p>
    {/if}
  {/if}
</div>

{#if confirmSave && editor.loaded}
  <Modal title={`Save to ${session.isComputer ? 'the library' : 'the device'}?`} onclose={() => (confirmSave = false)}>
    <p>This changes the files and database of the open library:</p>
    <ul class="what">
      {#if editor.gridDirty && editor.doc}
        <li>
          Replaces the beat grid of <strong>{label(editor.loaded)}</strong> ({strPath}): {editor.doc.beats.length} beats at
          {editor.bpm.toFixed(2)} BPM, first beat {signed(editor.offsetMs)} ms from the saved one.
          {#if vtempo.sections.length > 1}
            The tempo changes along the track, and the file stores a position for every beat, so it is saved as it is:
            <ul class="sections">
              {#each sectionLines(vtempo.sections, editor.sampleRate) as line (line)}<li>{line}</li>{/each}
            </ul>
          {/if}
        </li>
      {/if}
      {#if editor.cueDirty}
        <li>
          Sets the cue point of <strong>{label(editor.loaded)}</strong> to
          {editor.cue === null ? "not set" : fmtTime((editor.cue ?? 0) / editor.sampleRate)}.
        </li>
      {/if}
    </ul>
    {#if needsCueTick}
      <p class="warn-box">
        Unit unverified: the cue is written as a position in source sample frames, which no device-written cue has
        confirmed. A wrong unit would put the cue at the wrong place on the Pacemaker.
      </p>
      <label class="tick"><input type="checkbox" bind:checked={cueOk} /> Write the cue anyway</label>
    {/if}
    {#if needsMixTick}
      <p class="warn-box">
        {usedByMixes.length === 1 ? "1 mix uses" : `${usedByMixes.length} mixes use`} this track:
        <strong>{usedByMixes.join(", ")}</strong>. A mix clip keeps the tempo the track had when the mix was made, so
        with the new grid {usedByMixes.length === 1 ? "that mix" : "those mixes"} will drift out of step and may need to
        be beatmatched again.
      </p>
      <label class="tick"><input type="checkbox" bind:checked={mixesOk} /> Save the grid anyway</label>
    {/if}
    <p class="muted">
      The previous grid cannot be restored from here. A copy of music.db is kept in the app's data folder before the
      first write of each session (Device, Backup).
    </p>
    {#snippet actions()}
      <Button data-autofocus onclick={() => (confirmSave = false)}>Cancel</Button>
      <Button variant="primary" disabled={(needsCueTick && !cueOk) || (needsMixTick && !mixesOk)} onclick={doSave}>{session.isComputer ? 'Save to library' : 'Save to device'}</Button>
    {/snippet}
  </Modal>
{/if}

{#if pendingOpen !== null}
  <Modal
    title="Discard unsaved changes?"
    onclose={() => {
      pendingOpen = null;
      pickerKey++;
    }}
  >
    <p>
      <strong>{editor.loaded ? label(editor.loaded) : "This track"}</strong> has grid or cue edits that are not saved.
      Opening another track discards them.
    </p>
    {#snippet actions()}
      <Button
        data-autofocus
        onclick={() => {
          pendingOpen = null;
          pickerKey++;
        }}>Keep editing</Button
      >
      <Button
        variant="primary"
        onclick={() => {
          const id = pendingOpen;
          pendingOpen = null;
          if (id !== null) open(id);
          pickerKey++;
        }}>Discard and open</Button
      >
    {/snippet}
  </Modal>
{/if}

<style>
  .grid {
    --g-low: var(--wave-low);
    --g-mid: var(--wave-mid);
    --g-high: var(--wave-high);
    --g-beat: var(--beat);
    --g-cue: var(--cue);
    padding: 12px;
    overflow: auto;
    height: 100%;
  }
  .bar,
  .tools,
  .transport {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
    margin-bottom: 10px;
  }
  .grp {
    display: flex;
    align-items: center;
    gap: 4px;
    padding-right: 10px;
    border-right: 1px solid var(--line);
  }
  .grp:last-child {
    border-right: none;
  }
  .spacer {
    flex: 1;
  }
  select {
    max-width: 380px;
  }
  .chip {
    font-size: 11px;
    padding: 1px 7px;
    border-radius: 9px;
    background: var(--accent-soft);
    color: var(--accent);
  }
  .chip.g {
    background: rgba(0, 255, 70, 0.1);
    color: var(--green);
  }
  .chip.bad {
    background: var(--bad-bg);
    color: var(--bad-text);
  }
  .lab {
    color: var(--muted);
    font-size: 12px;
  }
  .bpm {
    width: 74px;
    text-align: right;
  }
  .lat {
    width: 60px;
  }
  .time {
    min-width: 76px;
  }
  .err {
    color: var(--bad);
    margin: 4px 0 8px;
  }
  .empty {
    color: var(--muted);
    padding: 24px;
  }
  .cols {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(260px, 1fr));
    gap: 14px;
    padding: 12px 0;
  }
  .card {
    background: var(--panel);
    border: 1px solid var(--line);
    border-radius: 6px;
    padding: 12px;
  }
  .card h4 {
    margin: 0 0 8px;
  }
  .field {
    display: grid;
    grid-template-columns: 96px 1fr;
    gap: 6px;
    align-items: center;
    margin: 6px 0;
  }
  .note {
    font-size: 12px;
    color: var(--muted);
    border-top: 1px solid var(--line);
    margin: 12px 0 0;
    padding-top: 10px;
  }
  .warn {
    color: var(--muted);
    margin: 0 0 10px;
  }
  .warn-box {
    border: 1px solid var(--line);
    border-left: 3px solid var(--g-cue);
    padding: 8px 10px;
    margin: 8px 0;
    font-size: 12px;
  }
  .tick {
    display: block;
    margin: 6px 0 10px;
  }
  .what {
    margin: 8px 0;
    padding-left: 18px;
  }
  .what li {
    margin: 6px 0;
  }
  .sections {
    max-height: 160px;
    overflow: auto;
    margin: 6px 0 0;
    padding-left: 18px;
    font-size: 12px;
    color: var(--muted);
  }
</style>
