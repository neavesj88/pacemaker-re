<script lang="ts">
  import type { Track } from "$lib/api/types";
  import { session } from "$lib/state/session.svelte";
  import Button from "$lib/ui/Button.svelte";
  import Modal from "$lib/ui/Modal.svelte";
  import type { Line } from "./matcher.svelte";
  import { searchTracks } from "./matchLogic";

  // Choose which library track a request line means: the matcher's candidates first, or search
  // the whole library by any words of the artist or title.
  let { line, onpick, onclose }: { line: Line; onpick: (trackId: number) => void; onclose: () => void } = $props();

  let query = $state("");
  const requested = $derived(line.item.artist ? `${line.item.artist} - ${line.item.title}` : line.item.title);

  type Option = { track: Track; score: number | null };
  const options = $derived.by<Option[]>(() => {
    const lib = session.library;
    if (!lib) return [];
    if (query.trim()) {
      const score = new Map(line.row.candidates.map((c) => [c.track_id, c.score]));
      return searchTracks(lib.tracks, query).map((track) => ({ track, score: score.get(track.track_id) ?? null }));
    }
    const out: Option[] = [];
    for (const c of line.row.candidates) {
      const track = lib.tracks.find((t) => t.track_id === c.track_id);
      if (track) out.push({ track, score: c.score });
    }
    return out;
  });
</script>

<Modal title="Which track is this?" {onclose}>
  <p class="req">Requested: <b>{requested}</b> <span class="muted">(line {line.item.line})</span></p>
  <!-- svelte-ignore a11y_autofocus -->
  <input data-autofocus bind:value={query} placeholder="Search your library by artist or title" aria-label="Search the library" />
  <p class="muted hint">
    {#if query.trim()}
      {options.length === 100 ? "The first 100 tracks that contain every word." : `${options.length} track${options.length === 1 ? "" : "s"} contain every word.`}
    {:else if options.length}
      The closest tracks the matcher found, best first. Type to search the whole library.
    {:else}
      The matcher found nothing close. Type to search the whole library.
    {/if}
  </p>
  <ul>
    {#each options as o (o.track.track_id)}
      <li>
        <button type="button" onclick={() => onpick(o.track.track_id)}>
          <span class="who">{o.track.artist || "(no artist)"} - {o.track.title || "(untitled)"}</span>
          {#if o.score !== null}<span class="score num">{o.score}</span>{/if}
        </button>
      </li>
    {/each}
  </ul>
  {#snippet actions()}
    <Button onclick={onclose}>Cancel</Button>
  {/snippet}
</Modal>

<style>
  input {
    width: 100%;
    box-sizing: border-box;
  }
  .req,
  .hint {
    margin: 8px 0 0;
  }
  ul {
    list-style: none;
    margin: 8px 0 0;
    padding: 0;
    max-height: 300px;
    overflow-y: auto;
    border: 1px solid var(--line);
    border-radius: 4px;
  }
  li + li {
    border-top: 1px solid var(--line);
  }
  li button {
    display: flex;
    justify-content: space-between;
    gap: 12px;
    width: 100%;
    padding: 6px 10px;
    background: none;
    border: none;
    color: var(--text);
    font: inherit;
    text-align: left;
    cursor: pointer;
  }
  li button:hover,
  li button:focus-visible {
    background: var(--hover);
    outline: none;
  }
  .score {
    color: var(--muted);
  }
  .num {
    font-variant-numeric: tabular-nums;
  }
</style>
