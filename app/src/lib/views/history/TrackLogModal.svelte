<script lang="ts">
  import { playlogTrack, type TrackLog } from "$lib/api/history";
  import type { Track } from "$lib/api/types";
  import { session } from "$lib/state/session.svelte";
  import { playlog } from "$lib/state/playlog.svelte";
  import Button from "$lib/ui/Button.svelte";
  import Modal from "$lib/ui/Modal.svelte";
  import { hitNote } from "./playlogLogic";
  import { ago, dayLabel, timeLabel } from "./when";

  // One track's play history from the log: the count it had when logging began, then each
  // logged play (newest first) with how sure each is.
  let { track, onclose }: { track: Track; onclose: () => void } = $props();

  let log = $state<TrackLog | null>(null);
  let loaded = $state(false);
  let error = $state("");

  $effect(() => {
    const root = session.library?.root;
    void playlog.tick;
    const id = track.track_id;
    if (!root) return;
    playlogTrack(root, id)
      .then((l) => (log = l))
      .catch((e) => (error = String(e)))
      .finally(() => (loaded = true));
  });

  const when = (ms: number) => new Date(ms).toLocaleString();
</script>

<Modal title="Play history" {onclose}>
  <p class="who"><b>{track.title}</b>{track.artist ? ` by ${track.artist}` : ""}</p>
  {#if error}
    <p class="err" role="alert">{error}</p>
  {:else if !loaded}
    <p class="muted">Reading the log...</p>
  {:else if !log}
    <p class="muted">There is no play log for this library, or this track is not in it.</p>
  {:else}
    <dl>
      <dt>Before the log</dt>
      <dd>
        {log.before_plays} play{log.before_plays === 1 ? "" : "s"}{log.before_last > 0 ? `, last played ${when(log.before_last)}` : ""}.
        <span class="muted">When the earlier plays happened is not known.</span>
      </dd>
      <dt>In the log</dt>
      <dd>{log.logged_plays} play{log.logged_plays === 1 ? "" : "s"}</dd>
      <dt>{session.isComputer ? "In the library now" : "On the device now"}</dt>
      <dd>{log.now_plays} play{log.now_plays === 1 ? "" : "s"}{log.now_last > 0 ? `, last played ${ago(log.now_last)}` : ""}</dd>
    </dl>
    {#if log.hits.length > 0}
      <table>
        <thead><tr><th>When</th><th class="num">Plays</th><th>Note</th></tr></thead>
        <tbody>
          {#each log.hits as h, i (i)}
            <tr>
              <td>{h.time_known ? "" : "~ "}{dayLabel(h.at)}, {timeLabel(h.at)}</td>
              <td class="num">{h.plays}{h.approx ? " ~" : ""}</td>
              <td class="muted">{hitNote(h) || "Exact."}</td>
            </tr>
          {/each}
        </tbody>
      </table>
    {:else}
      <p class="muted">No plays of this track have been logged since logging began.</p>
    {/if}
  {/if}
  {#snippet actions()}
    <Button data-autofocus onclick={onclose}>Close</Button>
  {/snippet}
</Modal>

<style>
  .who {
    margin: 0 0 10px;
  }
  .err {
    color: var(--bad);
  }
  dl {
    display: grid;
    grid-template-columns: max-content 1fr;
    gap: 4px 14px;
    margin: 0 0 12px;
  }
  dt {
    color: var(--muted);
  }
  dd {
    margin: 0;
  }
  table {
    width: 100%;
    border-collapse: collapse;
    font-size: 13px;
  }
  th,
  td {
    text-align: left;
    padding: 4px 8px;
    border-bottom: 1px solid var(--line);
    vertical-align: top;
  }
  .num {
    text-align: right;
  }
</style>
