<script lang="ts">
  // The two settings of the key detector (replace existing keys, confidence floor), remembered
  // between runs. Used by the "Detect keys" dialog, which passes how many of the chosen tracks
  // already have a key, and by the Settings view, which has no tracks and leaves it out.
  import { analysis } from "$lib/state/analysis.svelte";

  let { withKey = 0 }: { withKey?: number } = $props();

  const noun = (n: number) => `${n} track${n === 1 ? "" : "s"}`;
</script>

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

<style>
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
</style>
