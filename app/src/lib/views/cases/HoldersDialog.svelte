<script lang="ts">
  // "Which cases hold this track": every case the track is in (with its place there), and the
  // smart cases whose rules match it now. Each row opens that case in the Cases view.
  import { holders } from "$lib/state/holders.svelte";
  import { session } from "$lib/state/session.svelte";
  import Button from "$lib/ui/Button.svelte";
  import Modal from "$lib/ui/Modal.svelte";
  import { holdingRows } from "./holdersLogic";

  const id = $derived(holders.trackId);
  const track = $derived(id === null ? undefined : session.library?.tracks.find((t) => t.track_id === id));
  const rows = $derived(holders.holding ? holdingRows(holders.holding, session.library?.kind) : { cases: [], smart: [] });
</script>

{#if id !== null}
  <Modal title={`Cases holding “${track?.title || "this track"}”`} onclose={() => holders.close()}>
    {#if holders.loading}
      <p class="muted">Looking…</p>
    {:else if holders.error}
      <p class="bad">{holders.error}</p>
    {:else}
      {#if rows.cases.length === 0}
        <p class="muted">No case holds this track.</p>
      {:else}
        <ul class="list" aria-label="Cases holding this track">
          {#each rows.cases as c (c.case_id)}
            <li>
              <button type="button" class="row" onclick={() => holders.goToCase(c.case_id)} title="Open this case">
                <span class="n">{c.name}</span>
                <span class="muted">{c.note}</span>
              </button>
            </li>
          {/each}
        </ul>
      {/if}
      {#if rows.smart.length > 0}
        <p class="sub">Smart cases whose rules match it now</p>
        <ul class="list" aria-label="Smart cases matching this track">
          {#each rows.smart as s (s.id)}
            <li>
              <button type="button" class="row" onclick={() => holders.goToSmart(s.id)} title="Open this smart case">
                <span class="n">{s.name}</span>
                <span class="muted">{s.note}</span>
              </button>
            </li>
          {/each}
        </ul>
      {/if}
    {/if}
    {#snippet actions()}
      <Button data-autofocus onclick={() => holders.close()}>Close</Button>
    {/snippet}
  </Modal>
{/if}

<style>
  p {
    margin: 0 0 8px;
  }
  .bad {
    color: var(--bad-text);
  }
  .sub {
    margin: 12px 0 4px;
    color: var(--muted);
    font-weight: 600;
  }
  .list {
    margin: 0 0 4px;
    padding: 0;
    list-style: none;
    max-height: 40vh;
    overflow-y: auto;
  }
  .row {
    display: flex;
    justify-content: space-between;
    gap: 12px;
    width: 100%;
    padding: 6px 8px;
    background: transparent;
    border: 0;
    color: inherit;
    font: inherit;
    text-align: left;
    cursor: pointer;
  }
  .row:hover,
  .row:focus-visible {
    background: var(--hover);
  }
  .n {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
