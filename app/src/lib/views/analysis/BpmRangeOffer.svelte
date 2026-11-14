<script lang="ts">
  // The offer to re-analyse tracks whose BPM lies outside the BPM range in force: every such track
  // is listed with its BPM now. Nothing is written here; "Re-analyse" goes on to the usual
  // confirmation (which names the tracks again and warns about mixes and hand-set grids).
  import { offerLine, offerText, offerTitle, plural, rangeLabel } from "$lib/state/bpmRangeLogic";
  import { bpmRange } from "$lib/state/bpmRange.svelte";
  import { jobs } from "$lib/state/jobs.svelte";
  import Button from "$lib/ui/Button.svelte";
  import Modal from "$lib/ui/Modal.svelte";

  const list = $derived(bpmRange.outside);
</script>

<Modal title={list.length === 0 ? "BPM range" : offerTitle(list.length, bpmRange.lo)} onclose={() => bpmRange.closeOffer()}>
  {#if list.length === 0}
    <p>Every analysed track in this library reads inside {rangeLabel(bpmRange.lo)}.</p>
  {:else}
    <p>{offerText(bpmRange.lo)}</p>
    <ul class="tracks" aria-label="Tracks to re-analyse">
      {#each list as t (t.track_id)}
        <li>{offerLine(t)}</li>
      {/each}
    </ul>
  {/if}
  {#snippet actions()}
    <Button data-autofocus onclick={() => bpmRange.closeOffer()}>{list.length === 0 ? "Close" : "Not now"}</Button>
    {#if list.length > 0}
      <Button variant="primary" disabled={!jobs.canWrite} onclick={() => bpmRange.reanalyse()}>Re-analyse {plural(list.length, "track")}…</Button>
    {/if}
  {/snippet}
</Modal>

<style>
  .tracks {
    margin: 8px 0;
    padding: 6px 10px 6px 24px;
    max-height: 40vh;
    overflow: auto;
    font-size: 12px;
    background: var(--panel-2, rgba(128, 128, 128, 0.08));
    border-radius: 4px;
  }
</style>
