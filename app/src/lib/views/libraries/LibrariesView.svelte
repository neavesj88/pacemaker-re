<script lang="ts">
  import { untrack } from "svelte";
  import { JOB_SYNC } from "$lib/api/sync";
  import { session } from "$lib/state/session.svelte";
  import Button from "$lib/ui/Button.svelte";
  import JobProgress from "../device/JobProgress.svelte";
  import CopyDialog from "./CopyDialog.svelte";
  import IntoCaseDialog from "./IntoCaseDialog.svelte";
  import { libs } from "./libraries.svelte";
  import LibraryPane from "./LibraryPane.svelte";

  // The Editor's two panes: the open library on the left, another library on the right. Pick tracks,
  // mixes or cases in one and drag them onto the other to copy them there (tracks dropped on a case
  // row of the Cases tab also go into that case). The Copy button and the row menu do the same by
  // click. Before anything is written the dialog names every item and what is done to it. A copy only
  // adds: nothing on the target is removed or overwritten (the engine clamps the Sync settings for it).

  const canSwap = $derived(!!session.library && !!libs.data("right") && !libs.busy);

  // coming back to the view: what the right pane held may be out of date (a Sync run, an import, a
  // drive plugged in or out), so it is dropped; a Pacemaker waits for its button again
  $effect(() => {
    untrack(() => libs.invalidate());
    void session.refreshVolumes().catch(() => {});
  });
</script>

<div class="page">
  <div class="top">
    <h2>Libraries</h2>
  </div>

  {#if libs.error}
    <p class="banner" role="alert">
      {libs.error}
      <Button small variant="ghost" onclick={() => (libs.error = "")}>Dismiss</Button>
    </p>
  {/if}

  <!-- a strip of fixed height once a copy has been asked for, so the panes do not move between the
       check, the progress and the result; before that it is just the how-to line, not an empty band
       (the one move happens after the drop, when the pointer is already released) -->
  <div class="strip" class:idle={!libs.running && !libs.planning && !libs.result}>
    {#if libs.running}
      <section class="card" aria-label="Copy progress">
        <JobProgress kind={JOB_SYNC} />
      </section>
    {:else if libs.planning}
      <p class="muted" role="status">Checking what would be copied…</p>
    {:else if libs.result}
      <p class="result" role="status">{libs.result}</p>
    {:else}
      <p class="muted lead">
        Drag tracks, mixes or cases across to copy them. You see the list before anything is written.
      </p>
    {/if}
  </div>

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
<IntoCaseDialog />

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
  .strip {
    /* room for the progress card (bar, label, Cancel); a minimum, not a cap, so a large OS font
       scale or a long result sentence grows the strip instead of being cut off */
    min-height: 84px;
    flex: none;
    display: flex;
    align-items: center;
  }
  .strip.idle {
    min-height: 0;
  }
  .strip > * {
    flex: 1;
    margin: 0;
    min-width: 0;
  }
  .lead {
    margin: 2px 0 0;
  }
  .card {
    border: 1px solid var(--line);
    border-radius: 6px;
    background: var(--panel);
    padding: 4px 14px 6px;
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
