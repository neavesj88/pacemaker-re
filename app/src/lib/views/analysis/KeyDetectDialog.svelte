<script lang="ts">
  // Step one of "Detect keys": the two settings of the detector (replace existing keys, confidence
  // floor), remembered between runs, and how many of the chosen tracks the run will analyse.
  // "Continue" goes to the usual write confirmation, which lists every track and what happens.
  import { analysis } from "$lib/state/analysis.svelte";
  import { detectionPlan } from "$lib/state/keyLogic";
  import Button from "$lib/ui/Button.svelte";
  import Modal from "$lib/ui/Modal.svelte";

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

  <label class="opt">
    <input type="checkbox" bind:checked={analysis.settings.overwrite} />
    <span>
      Replace keys that are already set
      <small class="muted">
        Off: a track that has a key, including one you set by hand, is left alone.
        {#if withKey > 0}{noun(withKey)} of these {withKey === 1 ? "has" : "have"} one.{/if}
      </small>
    </span>
  </label>

  <label class="opt">
    <input type="number" min="0" max="100" step="5" bind:value={analysis.settings.minConfidencePct} aria-label="Confidence floor in per cent" />
    <span>
      Confidence floor, per cent
      <small class="muted">
        A result below this is skipped and listed when the job ends, not written. 0 writes every result. The score is a
        fit measure, not a probability.
      </small>
    </span>
  </label>

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
  .opt {
    display: flex;
    gap: 10px;
    align-items: flex-start;
    margin-bottom: 12px;
  }
  .opt input[type="number"] {
    width: 64px;
    flex: none;
  }
  .opt input[type="checkbox"] {
    flex: none;
    margin-top: 3px;
  }
  small {
    display: block;
    margin-top: 2px;
  }
  .count {
    color: var(--text);
  }
</style>
