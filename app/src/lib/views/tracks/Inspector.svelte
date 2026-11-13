<script lang="ts">
  // The right-hand panel: the track last clicked, with its overview waveform, the facts the
  // Pacemaker shows, and what mixes well with it.
  import type { Peak } from "$lib/api/analysis";
  import type { Track } from "$lib/api/types";
  import { mmss } from "$lib/format";
  import { labelEditor, labels } from "$lib/state/labels.svelte";
  import { suspectKinds } from "$lib/state/batchLogic";
  import { analysis } from "$lib/state/analysis.svelte";
  import { gridBatch } from "$lib/state/gridBatch.svelte";
  import { suspects } from "$lib/state/gridSuspects.svelte";
  import { jobs } from "$lib/state/jobs.svelte";
  import { playback } from "$lib/state/playback.svelte";
  import { session } from "$lib/state/session.svelte";
  import { tempo } from "$lib/state/tempo.svelte";
  import Button from "$lib/ui/Button.svelte";
  import Stars from "$lib/ui/Stars.svelte";
  import ColourSwatch from "./ColourSwatch.svelte";
  import EnergyBar from "./EnergyBar.svelte";
  import CaseChips from "../cases/CaseChips.svelte";
  import { cachedPeaks, requestPeaks } from "./peakCache";
  import { addToCase } from "./addToCase.svelte";
  import { addToMix, editGrid, editMetadata, playTrack, rate, revealInExplorer } from "./trackActions";
  import { keyColour, keyName, matchingTracks, parseKey } from "./trackLogic";
  import { tracksState } from "./tracksState.svelte";
  import { drawPeaks } from "./waveDraw";

  let {
    track,
    selectedIds,
    root,
    onpick,
  }: {
    track: Track | null;
    /** Everything selected, the shown track included: the buttons act on all of them. */
    selectedIds: number[];
    root: string;
    /** Select a track from the matching list. */
    onpick: (id: number) => void;
  } = $props();

  /** Why the grid area's rules flag this track, when they do. */
  const suspect = $derived(track ? suspects.byId.get(track.track_id) : undefined);

  const OVERVIEW_COLUMNS = 400;
  const MATCH_LIMIT = 8;

  let canvas = $state<HTMLCanvasElement | null>(null);
  let width = $state(0);
  let peaks = $state.raw<Peak[] | null>(null);

  $effect(() => {
    const loc = track?.location;
    if (!loc) {
      peaks = null;
      return;
    }
    const cached = cachedPeaks(root, loc, OVERVIEW_COLUMNS);
    if (cached !== undefined) {
      peaks = cached;
      return;
    }
    peaks = null;
    return requestPeaks(root, loc, OVERVIEW_COLUMNS, (p) => (peaks = p));
  });

  $effect(() => {
    if (canvas && width > 0) drawPeaks(canvas, peaks, width, 72);
  });

  const camelot = $derived(track ? parseKey(track.key) : null);
  const needs = $derived(track ? session.needs.get(track.track_id) : undefined);
  const matches = $derived(track && !needs && session.library ? matchingTracks(track, session.library.tracks, 4, session.needs) : []);
  const loaded = $derived(!!track && playback.trackId === track.track_id && playback.duration > 0);
  const many = $derived(selectedIds.length > 1);
  const lab = $derived(track ? labels.get(track.track_id) : undefined);
  const fav = $derived(!!track && tracksState.favourites.has(track.track_id));
  const allFav = $derived(selectedIds.length > 0 && selectedIds.every((id) => tracksState.favourites.has(id)));

  function seek(e: MouseEvent) {
    if (!loaded) return;
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    playback.seek(Math.min(1, Math.max(0, (e.clientX - r.left) / r.width)) * playback.duration);
  }

  const megabytes = (bytes: number) => `${(bytes / 1_000_000).toFixed(1)} MB`;
</script>

<aside class="insp" aria-label="Track details">
  {#if !track}
    <p class="empty">Select a track to see its details and the tracks that mix well with it.</p>
  {:else}
    <h3 title={track.title}>{track.title}</h3>
    <div class="muted sub">{[track.artist, track.genre].filter(Boolean).join(" · ") || "No artist or genre"}</div>
    {#if many}<div class="chip">{selectedIds.length} tracks selected, showing the last one clicked</div>{/if}

    <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
    <div class="wave" class:seekable={loaded} bind:clientWidth={width} onclick={seek} title={loaded ? "Click to seek" : ""}>
      <canvas bind:this={canvas} style:width="{width}px" style:height="72px" aria-label="Waveform overview"></canvas>
      {#if loaded}<span class="head" style:left="{(playback.position / playback.duration) * 100}%"></span>{/if}
    </div>

    <div class="field">
      <span class="label">BPM</span>
      <span>
        {track.bpm && !needs ? track.bpm.toFixed(2) : "–"}
        {#if needs}<span class="chip warn" title={needs}>needs analysis</span>{:else if track.analysed}<span class="chip ok">grid checked</span>{/if}
        <Button
          variant="ghost"
          small
          disabled={many || !!needs}
          onclick={() => tempo.askSet(track.track_id)}
          title={needs ? "Re-analyse this track first: it has no readable beat grid" : many ? "Select one track to type its BPM" : "Type the BPM; the beat grid is rewritten with it"}
        >Edit</Button>
      </span>
    </div>
    {#if suspect}
      <div class="field">
        <span class="label">Grid</span>
        <span>
          <span class="chip warn" title={suspect.reasons.join("\n")}>suspect</span>
          {suspectKinds(suspect)}
          <Button variant="ghost" small onclick={() => gridBatch.open([suspect.track_id], suspect.fix ? "suggested" : "reanalyse")} title={suspect.reasons.join("; ")}>
            {suspect.fix ? "Preview fix…" : "Preview re-analysis…"}
          </Button>
        </span>
      </div>
    {/if}
    <div class="field">
      <span class="label">Key</span>
      <span>
        {#if camelot}
          <span class="key" style:color={keyColour(camelot)}>{camelot.code}</span> (Camelot) · {keyName(camelot)}
        {:else if track.key}
          {track.key} <span class="muted">(not a key Camelot covers)</span>
        {:else}
          <span class="muted">unknown</span>
        {/if}
        <Button variant="ghost" small onclick={() => analysis.askSetKey(selectedIds)} title="Set the key by hand{many ? ` of all ${selectedIds.length} selected tracks` : ''}">Edit</Button>
      </span>
    </div>
    <div class="field">
      <span class="label">Rating</span>
      <Stars rating={track.rating} onchange={(n) => rate(track, n)} />
    </div>
    <div class="field">
      <span class="label" title="Kept by this app, not on the Pacemaker">Labels</span>
      <span class="labs">
        {#if lab}
          <ColourSwatch colour={lab.colour} size={14} />
          <EnergyBar energy={lab.energy} />
          {#each lab.tags as tag (tag)}<span class="chip tagc">{tag}</span>{/each}
        {:else}
          <span class="muted">none</span>
        {/if}
      </span>
    </div>
    <div class="field">
      <span class="label">Played</span>
      <span>{track.times_played === 0 ? "never" : track.times_played === 1 ? "1 time" : `${track.times_played} times`}</span>
    </div>
    <div class="field"><span class="label">Genre</span><span>{track.genre || "–"}</span></div>
    <div class="field"><span class="label">Length</span><span>{mmss(track.play_time_secs)}</span></div>
    <div class="field"><span class="label">File</span><span>{[track.format, track.file_size ? megabytes(track.file_size) : ""].filter(Boolean).join(" · ") || "–"}</span></div>
    <div class="field"><span class="label">In cases</span><CaseChips trackId={track.track_id} /></div>

    <div class="buttons">
      <Button variant="primary" small onclick={() => playTrack(track.track_id)}>Play</Button>
      <Button small onclick={() => editMetadata(selectedIds)}>{many ? `Edit ${selectedIds.length} tracks…` : "Edit metadata…"}</Button>
      <Button small onclick={() => labelEditor.edit(selectedIds.length ? selectedIds : [track.track_id])}>{many ? `Tags for ${selectedIds.length}…` : "Tags, colour, energy…"}</Button>
      <Button small onclick={() => editGrid(track.track_id)}>Edit grid</Button>
      <Button small active={many ? allFav : fav} onclick={() => tracksState.setFavourite(selectedIds, !(many ? allFav : fav))}>
        ♥ {(many ? allFav : fav) ? "Unfavourite" : "Favourite"}
      </Button>
      <Button small onclick={() => addToCase.ask(selectedIds)}>{many ? `Add ${selectedIds.length} to case…` : "Add to case…"}</Button>
      <Button small onclick={() => addToMix(selectedIds)}>{many ? `Add ${selectedIds.length} to mix` : "Add to mix"}</Button>
      <Button small onclick={() => revealInExplorer(track.track_id)}>Show in Explorer</Button>
      <Button
        small
        disabled={!jobs.canWrite}
        onclick={() => jobs.askReanalyse(selectedIds)}
        title="Work out the beat grid and BPM again from the audio"
      >{many ? `Re-analyse ${selectedIds.length}…` : "Re-analyse…"}</Button>
    </div>

    <div class="note">
      <div class="note-head">Matching tracks <span class="muted">(compatible key, BPM within ±4%)</span></div>
      {#if !camelot || !track.bpm || needs}
        <span class="muted">Needs a known key and BPM.</span>
      {:else if matches.length === 0}
        <span class="muted">None in this library.</span>
      {:else}
        <ul>
          {#each matches.slice(0, MATCH_LIMIT) as m (m.track_id)}
            {@const mk = parseKey(m.key)}
            <li>
              <button class="match" onclick={() => onpick(m.track_id)} title="Select {m.title}">
                <span class="mt">{m.title}</span>
                <span class="muted mm">{m.bpm.toFixed(1)}{#if mk}{" · "}<span style:color={keyColour(mk)}>{mk.code}</span>{/if}</span>
              </button>
            </li>
          {/each}
        </ul>
        {#if matches.length > MATCH_LIMIT}<span class="muted">and {matches.length - MATCH_LIMIT} more</span>{/if}
      {/if}
    </div>
  {/if}
</aside>

<style>
  @media (max-width: 1100px) {
    .insp {
      width: 220px;
    }
  }
  .insp {
    width: 300px;
    flex: none;
    box-sizing: border-box;
    overflow: auto;
    padding: 12px;
    background: var(--panel);
    border-left: 1px solid var(--line);
  }
  h3 {
    margin: 0 0 2px;
    font-size: 14px;
    overflow-wrap: anywhere;
  }
  .sub {
    margin-bottom: 8px;
  }
  .wave {
    position: relative;
    margin: 8px 0 10px;
    height: 72px;
    background: var(--deep);
    border: 1px solid var(--line);
    border-radius: 4px;
    overflow: hidden;
  }
  .wave.seekable {
    cursor: pointer;
  }
  canvas {
    display: block;
  }
  .head {
    position: absolute;
    top: 0;
    bottom: 0;
    width: 2px;
    background: var(--text);
    pointer-events: none;
  }
  .field {
    display: grid;
    grid-template-columns: 70px 1fr;
    gap: 6px;
    align-items: center;
    margin: 6px 0;
  }
  .label {
    color: var(--muted);
    font-size: 12px;
  }
  .chip {
    display: inline-block;
    padding: 1px 7px;
    border-radius: 9px;
    font-size: 11px;
    background: var(--accent-soft);
    color: var(--accent);
  }
  .chip.ok {
    background: color-mix(in srgb, var(--green) 10%, transparent);
    color: var(--green);
  }
  .key {
    font-weight: 600;
  }
  .labs {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
  }
  .chip.tagc {
    background: var(--hover);
    color: var(--text);
  }
  .buttons {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-top: 10px;
  }
  .note {
    margin-top: 12px;
    padding-top: 10px;
    border-top: 1px solid var(--line);
    font-size: 12px;
  }
  .note-head {
    margin-bottom: 4px;
  }
  ul {
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .match {
    display: flex;
    justify-content: space-between;
    gap: 8px;
    width: 100%;
    padding: 3px 6px;
    background: none;
    border: none;
    border-radius: 4px;
    color: var(--text);
    font: inherit;
    text-align: left;
    cursor: pointer;
  }
  .match:hover {
    background: var(--hover);
  }
  .mt {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .mm {
    flex: none;
  }
</style>
