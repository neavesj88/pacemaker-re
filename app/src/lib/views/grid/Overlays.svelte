<script lang="ts">
  // Always-mounted parts of the grid area: the "Set BPM" dialog (opened by `tempo.askSet`), the batch
  // grid dialog (`gridBatch.open`) and the suspect-grid rules (`suspects.editing`). Also keeps the
  // suspect check in step with the open library.
  import { gridBatch } from "$lib/state/gridBatch.svelte";
  import { suspects } from "$lib/state/gridSuspects.svelte";
  import { jobs } from "$lib/state/jobs.svelte";
  import { session } from "$lib/state/session.svelte";
  import { tempo } from "$lib/state/tempo.svelte";
  import BatchDialog from "./BatchDialog.svelte";
  import BpmDialog from "./BpmDialog.svelte";
  import SuspectDialog from "./SuspectDialog.svelte";

  // a library opened or re-read (after any job) is checked again
  $effect(() => {
    suspects.sync(session.library);
  });

  // the write confirmation went away: the batch write started (close) or was cancelled (show again)
  $effect(() => {
    if (!jobs.pending) gridBatch.confirmationGone();
  });
</script>

{#if tempo.settingId !== null}
  <BpmDialog trackId={tempo.settingId} />
{/if}
{#if gridBatch.shown && !jobs.pending}
  <BatchDialog />
{/if}
{#if suspects.editing}
  <SuspectDialog />
{/if}
