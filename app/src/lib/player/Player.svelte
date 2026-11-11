<script lang="ts">
  // The bottom player bar. Owned by the playback area; the shell only mounts it. It shows the
  // transport (state/playback.svelte.ts) and sends clicks back to it or to the engine.
  import { playback } from "$lib/state/playback.svelte";
  import { session } from "$lib/state/session.svelte";
  import Button from "$lib/ui/Button.svelte";
  import { engine } from "./engine.svelte";
  import PlayerWave from "./PlayerWave.svelte";

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

<footer class="player" aria-label="Player">
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
      aria-label={playback.playing ? "Pause" : "Play"}
      title="Play / pause (Space)"
      disabled={!loaded}
      onclick={() => playback.toggle()}>{playback.playing ? "⏸" : "▶"}</Button
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
  .player {
    display: grid;
    grid-template-columns: minmax(160px, 260px) 1fr auto;
    gap: 12px;
    align-items: center;
    padding: 6px 14px;
    background: var(--deep);
    border-top: 1px solid var(--line);
    font-size: 12px;
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
    font-size: 11px;
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
    gap: 6px;
  }
  .time {
    min-width: 92px;
    margin-right: 4px;
    color: var(--green);
    font: 600 13px ui-monospace, Consolas, monospace;
    text-align: left;
  }
  .vol {
    width: 80px;
    padding: 0;
    border: none;
    background: none;
    accent-color: var(--accent);
  }
  @media (max-width: 900px) {
    .player {
      grid-template-columns: 1fr auto;
    }
    .player :global(.wave) {
      grid-column: 1 / -1;
      order: 3;
    }
  }
</style>
