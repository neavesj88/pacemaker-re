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
      Off: tracks with a key are left alone.
      {#if withKey > 0}{noun(withKey)} of these {withKey === 1 ? "has" : "have"} one.{/if}
    </small>
  </span>
</label>

<label class="opt">
  <input type="number" min="0" max="100" step="5" bind:value={analysis.settings.minConfidencePct} aria-label="Confidence floor in per cent" />
  <span>
    Confidence floor, per cent
    <small class="muted">
      Lower results are skipped and listed. 0 writes every result.
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
