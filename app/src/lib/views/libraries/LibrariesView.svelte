<script lang="ts">
  import { JOB_SYNC } from "$lib/api/sync";
  import { session } from "$lib/state/session.svelte";
  import Button from "$lib/ui/Button.svelte";
  import { device } from "../device/device.svelte";
  import JobProgress from "../device/JobProgress.svelte";
  import CopyDialog from "./CopyDialog.svelte";
  import { libs } from "./libraries.svelte";
  import LibraryPane from "./LibraryPane.svelte";

  // The Editor's two panes: the open library on the left, another library on the right. Pick tracks,
  // mixes or cases in one and drag them onto the other to copy them there (tracks dropped on a case
  // row of the Cases tab also go into that case). The Copy button and the row menu do the same by
  // click. Before anything is written the dialog names every item and what is done to it; nothing is
  // ever removed from either library.

  const canSwap = $derived(!!session.library && !!libs.data("right") && !libs.busy);

  $effect(() => {
    void session.refreshVolumes().catch(() => {});
  });
</script>

<div class="page">
  <div class="top">
    <h2>Libraries</h2>
    <p class="muted lead">
      Drag tracks, mixes or cases from one side to the other to copy them. You see what will be copied before anything is written.
    </p>
  </div>

  {#if libs.error}
    <p class="banner" role="alert">
      {libs.error}
      <Button small variant="ghost" onclick={() => (libs.error = "")}>Dismiss</Button>
    </p>
  {/if}

  {#if libs.planning}
    <p class="muted" role="status">Checking what would be copied…</p>
  {/if}

  {#if libs.running || device.job?.kind === JOB_SYNC}
    <section class="card" aria-label="Copy progress">
      <h3>Copying…</h3>
      <JobProgress kind={JOB_SYNC} />
    </section>
  {:else if libs.result}
    <p class="result" role="status">{libs.result}</p>
  {/if}

  <div class="panes">
    <LibraryPane side="left" />
    <div class="mid">
      <Button
        title="Open the library on the right as the open library; the open one moves to the right"
        aria-label="Swap the two libraries"
        disabled={!canSwap}
        onclick={() => libs.swap()}
      >⇄</Button>
    </div>
    <LibraryPane side="right" />
  </div>
</div>

<CopyDialog />

<style>
  .page {
    padding: 14px 16px 16px;
    display: flex;
    flex-direction: column;
    gap: 10px;
    min-width: 0;
    height: 100%;
    box-sizing: border-box;
    overflow: auto;
  }
  .top h2 {
    margin: 0;
  }
  h3 {
    margin: 0;
    font-size: 14px;
  }
  .lead {
    margin: 2px 0 0;
  }
  .card {
    border: 1px solid var(--line);
    border-radius: 6px;
    background: var(--panel);
    padding: 10px 14px;
  }
  .result {
    margin: 0;
    padding: 6px 10px;
    border-left: 3px solid var(--ok);
    background: var(--row-alt);
    font-size: 13px;
  }
  .panes {
    flex: 1;
    min-height: 330px;
    display: grid;
    grid-template-columns: minmax(0, 1fr) 34px minmax(0, 1fr);
    gap: 0;
    align-items: stretch;
  }
  .mid {
    display: flex;
    align-items: flex-start;
    justify-content: center;
    padding-top: 8px;
  }
  .banner {
    margin: 0;
    padding: 8px 12px;
    border: 1px solid var(--bad);
    border-radius: 6px;
    background: var(--bad-bg);
    color: var(--bad-text);
    display: flex;
    gap: 10px;
    align-items: center;
    flex-wrap: wrap;
  }
</style>
