<script lang="ts">
  // Step one of "Detect keys": the two settings of the detector (replace existing keys, confidence
  // floor), remembered between runs, and how many of the chosen tracks the run will analyse.
  // "Continue" goes to the usual write confirmation, which lists every track and what happens.
  import { analysis } from "$lib/state/analysis.svelte";
  import { detectionPlan } from "$lib/state/keyLogic";
  import Button from "$lib/ui/Button.svelte";
  import Modal from "$lib/ui/Modal.svelte";
  import KeySettingsFields from "./KeySettingsFields.svelte";

  let { ids }: { ids: number[] } = $props();

  const s = $derived(analysis.settings);
  const plan = $derived(detectionPlan(analysis.tracksOf(ids), s));
  const withKey = $derived(analysis.tracksOf(ids).filter((t) => t.key.trim()).length);
  const noun = (n: number) => `${n} track${n === 1 ? "" : "s"}`;
</script>

<Modal title={ids.length === 1 ? "Detect the key" : `Detect the keys of ${ids.length} tracks`} onclose={() => analysis.closeDialog()}>
  <p class="muted">
    The key is worked out from the audio of each track and written to the library's key field. These settings are
    remembered for the next run.
  </p>

  <KeySettingsFields {withKey} />

  <p class="count" role="status">
    {#if plan.targets.length === 0}
      Nothing to do: every one of these tracks has a key and replacing is off.
    {:else}
      {noun(plan.targets.length)} will be analysed{plan.kept.length > 0 ? `, ${noun(plan.kept.length)} keep${plan.kept.length === 1 ? "s" : ""} the key ${plan.kept.length === 1 ? "it has" : "they have"}` : ""}.
    {/if}
  </p>

  {#snippet actions()}
    <Button data-autofocus onclick={() => analysis.closeDialog()}>Cancel</Button>
    <Button variant="primary" disabled={plan.targets.length === 0} onclick={() => analysis.continueDetect()}>Continue…</Button>
  {/snippet}
</Modal>

<style>
  p {
    margin: 0 0 12px;
  }
  .count {
    color: var(--text);
  }
</style>
