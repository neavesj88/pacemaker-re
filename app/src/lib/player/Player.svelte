<script lang="ts">
  // The bottom player bar. Owned by the playback area; the shell only mounts it. It shows the
  // transport (state/playback.svelte.ts) and sends clicks back to it or to the engine.
  import { playback } from "$lib/state/playback.svelte";
  import { session } from "$lib/state/session.svelte";
  import { loadPref, savePref } from "$lib/state/prefStore";
  import Button from "$lib/ui/Button.svelte";
  import { engine } from "./engine.svelte";
  import PlayerWave from "./PlayerWave.svelte";
  import {
    DEFAULT_HEIGHT,
    MIN_HEIGHT,
    PLAYER_HEIGHT_KEY,
    clampHeight,
    dragHeight,
    keyHeight,
    maxHeight,
    normaliseHeight,
    scaleFor,
  } from "./playerSize";

  const track = $derived(
    playback.trackId === null ? null : (session.library?.tracks.find((t) => t.track_id === playback.trackId) ?? null),
  );
  const mix = $derived(playback.mixPreview);
  const loaded = $derived(track !== null || mix !== null);

  /** Seconds the waveform spans: the file's length. */
  const span = $derived(playback.duration || track?.play_time_secs || 0);
  /** Position within the file (a mix preview's `position` is on the mix timeline). */
  const local = $derived(Math.max(0, playback.position - (mix?.fromSecs ?? 0)));
  const progress = $derived(span > 0 ? Math.min(1, local / span) : 0);

  const clock = (s: number) => {
    const t = Math.max(0, Math.floor(s));
    return `${Math.floor(t / 60)}:${String(t % 60).padStart(2, "0")}`;
  };

  const meta = $derived(
    track
      ? [
          track.artist,
          track.bpm && track.analysed ? `${track.bpm.toFixed(2)} BPM` : "",
          track.key,
        ]
          .filter(Boolean)
          .join(" · ")
      : "",
  );

  /** The one status line under the title, worst first. */
  const status = $derived.by((): { text: string; bad: boolean } | null => {
    if (playback.error) return { text: playback.error, bad: true };
    if (track && engine.peaksProblem) return { text: `No waveform: ${engine.peaksProblem}`, bad: false };
    if (track && engine.beatsProblem) return { text: `No beat ticks: ${engine.beatsProblem}`, bad: false };
    if (loaded && engine.silent) return { text: "Browser preview: no audio, silent clock", bad: false };
    return null;
  });

  // Bar height. `chosen` is what the owner set and what is remembered; `barH` is that held to the
  // current window (45 % of its height at most), so a size chosen on a big screen fits a small one
  // and comes back intact. Read here, in the component's setup, so the first paint already has it.
  let chosen = $state(loadPref(PLAYER_HEIGHT_KEY, normaliseHeight, () => DEFAULT_HEIGHT));
  let winW = $state(typeof window === "undefined" ? 1920 : window.innerWidth);
  let winH = $state(typeof window === "undefined" ? 1080 : window.innerHeight);
  const barH = $derived(clampHeight(chosen, winH));
  const scale = $derived(scaleFor(barH, winW));
  let resizing = $state(false);
  let drag: { y: number; h: number; start: number; id: number; el: HTMLElement } | null = null;

  function setChosen(h: number) {
    chosen = h;
    savePref(PLAYER_HEIGHT_KEY, h);
  }
  function gripDown(e: PointerEvent) {
    if (e.button !== 0) return;
    const el = e.currentTarget as HTMLElement;
    drag = { y: e.clientY, h: barH, start: chosen, id: e.pointerId, el };
    resizing = true;
    el.setPointerCapture(e.pointerId);
  }
  // live while dragging; saved once on release so storage is not written per pixel
  function gripMove(e: PointerEvent) {
    if (drag) chosen = dragHeight(drag.h, drag.y, e.clientY, winH);
  }
  function gripUp() {
    if (!drag) return;
    drag = null;
    resizing = false;
    setChosen(chosen);
  }
  function gripKey(e: KeyboardEvent) {
    if (e.key === "Escape" && drag) {
      // cancel: back to the size from before the drag, nothing saved
      const d = drag;
      drag = null;
      resizing = false;
      chosen = d.start;
      if (d.el.hasPointerCapture(d.id)) d.el.releasePointerCapture(d.id);
      return;
    }
    if (e.ctrlKey || e.metaKey || e.altKey) return;
    const h = keyHeight(e.key, e.shiftKey, barH, winH);
    if (h === null) return;
    e.preventDefault();
    setChosen(h);
  }

  // The bar's real height (it grows past --ph when the layout wraps), published for the toasts,
  // which sit just above it.
  let barPx = $state(0);
  $effect(() => {
    if (typeof document === "undefined" || barPx <= 0) return;
    document.documentElement.style.setProperty("--player-h", `${barPx}px`);
    return () => document.documentElement.style.removeProperty("--player-h");
  });

  const seekFraction = (f: number) => playback.seek((mix?.fromSecs ?? 0) + f * span);

  function cueDown(e: PointerEvent) {
    if (e.shiftKey) engine.setCueHere();
    else engine.cuePress();
  }

  // Keyboard CUE on the focused button: Enter or Space, held like the pointer (Shift sets the cue)
  function cueKeyDown(e: KeyboardEvent) {
    if ((e.key !== "Enter" && e.key !== " ") || e.repeat) return;
    e.preventDefault();
    if (e.shiftKey) engine.setCueHere();
    else engine.cuePress();
  }
  function cueKeyUp(e: KeyboardEvent) {
    if (e.key !== "Enter" && e.key !== " ") return;
    e.preventDefault();
    engine.cueRelease();
  }
</script>

<svelte:window bind:innerWidth={winW} bind:innerHeight={winH} />

<footer class="player" class:resizing aria-label="Player" bind:offsetHeight={barPx} style="--ph: {barH}px; --ps: {scale}">
  <!-- Drag up to make the bar taller. Double-click resets; Up / Down step, Home / End = min / max.
       A focusable separator is the ARIA window-splitter pattern; svelte only knows it as non-interactive. -->
  <!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
  <div
    class="grip"
    role="separator"
    aria-orientation="horizontal"
    aria-label="Player height"
    aria-valuemin={MIN_HEIGHT}
    aria-valuemax={maxHeight(winH)}
    aria-valuenow={barH}
    tabindex="0"
    title="Drag up or down to resize the player. Double-click for the default size."
    onpointerdown={gripDown}
    onpointermove={gripMove}
    onpointerup={gripUp}
    onpointercancel={gripUp}
    onlostpointercapture={gripUp}
    onkeydown={gripKey}
    ondblclick={() => setChosen(DEFAULT_HEIGHT)}
  >
    <span class="grip-line"></span>
  </div>
  <div class="info">
    {#if track}
      <b class="title" title={track.title}>{track.title}</b>
      <div class="muted line" title={meta}>{meta || " "}</div>
    {:else if mix}
      <b class="title" title={mix.label}>{mix.label}</b>
      <div class="muted line">Mix preview</div>
    {:else}
      <b class="title muted">Nothing loaded</b>
      <div class="muted line">Select a track and press Space</div>
    {/if}
    {#if status}<div class="line status" class:bad={status.bad} title={status.text}>{status.text}</div>{/if}
  </div>

  <PlayerWave
    peaks={mix ? null : engine.peaks}
    beats={mix ? [] : engine.beats}
    firstDownbeat={engine.firstDownbeat}
    duration={span}
    {progress}
    cue={track && engine.ready ? playback.cue : null}
    loop={playback.loop}
    busy={engine.loading}
    disabled={!loaded || !engine.ready}
    onseek={seekFraction}
  />

  <div class="controls">
    <span class="time num" aria-label="Position">{clock(local)} / {clock(span)}</span>
    <Button
      small
      aria-label="Previous track"
      title="Previous track (restarts the track after 3 seconds)"
      disabled={!track}
      onclick={() => playback.previous()}>⏮</Button
    >
    <Button
      small
      aria-label="Cue"
      title="Cue: at the cue point, hold to preview; elsewhere, jump to the cue point. Shift-click sets the cue point here. Key: C"
      disabled={!track || !engine.ready}
      onpointerdown={cueDown}
      onkeydown={cueKeyDown}
      onkeyup={cueKeyUp}
      onpointerup={() => engine.cueRelease()}
      onpointerleave={() => engine.cueRelease()}
      onpointercancel={() => engine.cueRelease()}>CUE</Button
    >
    <Button
      small
      variant="primary"
      aria-label={playback.error ? "Try again" : playback.playing ? "Pause" : "Play"}
      title={playback.error ? "Open the file again and play it" : "Play / pause (Space)"}
      disabled={!loaded}
      onclick={() => (playback.error ? playback.retry() : playback.toggle())}>{playback.error ? "↻" : playback.playing ? "⏸" : "▶"}</Button
    >
    <Button
      small
      active={playback.loop !== null}
      aria-pressed={playback.loop !== null}
      aria-label="4-beat loop"
      title="4-beat loop from the last beat"
      disabled={!track || !engine.ready}
      onclick={() => engine.toggleLoop()}>⟲ 4</Button
    >
    <Button
      small
      aria-label="Next track"
      title="Next track"
      disabled={playback.neighbour(1) === null}
      onclick={() => playback.next()}>⏭</Button
    >
    <input
      class="vol"
      type="range"
      min="0"
      max="1"
      step="0.01"
      aria-label="Volume"
      title="Volume"
      value={playback.volume}
      oninput={(e) => playback.setVolume(+e.currentTarget.value)}
    />
  </div>
</footer>

<style>
  /* --ph is the bar height and --ps the content scale (1 at the default size), both set from the
     script; every size below is its 1x value times --ps, so the contents grow in steps with the bar */
  .player {
    --ps: 1;
    position: relative;
    flex: none;
    box-sizing: border-box;
    min-height: var(--ph, 63px);
    display: grid;
    grid-template-columns: minmax(calc(160px * var(--ps)), calc(260px * var(--ps))) 1fr auto;
    gap: calc(12px * var(--ps));
    align-items: center;
    padding: calc(6px * var(--ps)) 14px 6px;
    background: var(--deep);
    border-top: 1px solid var(--line);
    font-size: calc(12px * var(--ps));
  }
  .grip {
    position: absolute;
    inset: 0 0 auto 0;
    /* grows with --ps and stays inside the 1px border + top padding (also 6px * --ps), so it never covers the waveform or the text */
    height: calc(6px * var(--ps));
    z-index: 2;
    display: flex;
    align-items: center;
    justify-content: center;
    cursor: row-resize;
    touch-action: none;
    user-select: none;
  }
  .grip-line {
    width: calc(44px * var(--ps));
    height: calc(3px * var(--ps));
    border-radius: calc(2px * var(--ps));
    background: var(--muted);
    transition: background 0.12s;
  }
  .grip:hover .grip-line,
  .grip:focus-visible .grip-line,
  .resizing .grip-line {
    background: var(--accent);
  }
  .grip:focus-visible {
    outline: 1px solid var(--accent);
    outline-offset: -1px;
  }
  .resizing {
    cursor: row-resize;
  }
  .info {
    min-width: 0;
  }
  .title,
  .line {
    display: block;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .line {
    font-size: calc(11px * var(--ps));
  }
  .status {
    color: var(--muted);
    font-style: italic;
  }
  .status.bad {
    color: var(--bad-text);
    font-style: normal;
  }
  .controls {
    display: flex;
    align-items: center;
    gap: calc(6px * var(--ps));
  }
  .controls :global(.btn) {
    padding: calc(2px * var(--ps)) calc(8px * var(--ps));
    font-size: calc(12px * var(--ps));
  }
  .time {
    min-width: calc(92px * var(--ps));
    margin-right: calc(4px * var(--ps));
    color: var(--green);
    font: 600 calc(13px * var(--ps)) ui-monospace, Consolas, monospace;
    text-align: left;
  }
  .vol {
    /* zoom scales the slider's track and thumb as well as its width */
    zoom: var(--ps);
    width: 80px;
    padding: 0;
    border: none;
    background: none;
    accent-color: var(--accent);
  }
  @media (max-width: 900px) {
    .player {
      grid-template-columns: 1fr auto;
      /* the waveform row takes all the height the bar gains */
      grid-template-rows: auto 1fr;
    }
    .player :global(.wave) {
      grid-column: 1 / -1;
      order: 3;
    }
  }
</style>
