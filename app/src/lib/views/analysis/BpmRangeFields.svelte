<script lang="ts">
  // The BPM range choice: the window every analysed tempo is doubled or halved into. Shown in the
  // Settings view (Analysis). Choosing a window saves it at once; if tracks of the open library
  // then lie outside it, the offer to re-analyse them opens. The button below the choices reaches
  // the same offer later.
  import { BPM_RANGES, plural } from "$lib/state/bpmRangeLogic";
  import { bpmRange } from "$lib/state/bpmRange.svelte";
  import { jobs } from "$lib/state/jobs.svelte";
  import Button from "$lib/ui/Button.svelte";

  const n = $derived(bpmRange.outside.length);
</script>

<fieldset>
  <legend class="sr">BPM range</legend>
  {#key bpmRange.failed}{#each BPM_RANGES as r (r.lo)}
    <label class="opt">
      <input type="radio" name="bpm-range" checked={bpmRange.lo === r.lo} onchange={() => bpmRange.choose(r.lo)} />
      <span>
        <b>{r.label}</b>{#if r.lo === 88}<span class="chip ok dflt">default</span>{/if}
        <small class="muted">{r.hint}</small>
      </span>
    </label>
  {/each}{/key}
</fieldset>
<p class="muted note">
  Applies to tracks analysed from now on: adding tracks, Re-analyse, the grid editor's Re-analyse and the grid batch.
  The device syncs by BPM, so tracks the original Editor analysed (drum &amp; bass at about 87) and tracks analysed here (about 174) can disagree.
</p>
<div class="offer">
  {#if n > 0}
    <Button disabled={!jobs.canWrite} onclick={() => bpmRange.openOffer()}>Re-analyse {plural(n, "track")} read with another range…</Button>
  {:else}
    <span class="muted">{bpmRange.loaded && 0 === n ? "No track in the open library reads outside this range." : ""}</span>
  {/if}
</div>

<style>
  fieldset {
    margin: 0 0 8px;
    padding: 0;
    border: none;
    display: grid;
    gap: 8px;
  }
  .sr {
    position: absolute;
    left: -9999px;
  }
  .opt {
    display: flex;
    gap: 10px;
    align-items: flex-start;
  }
  .opt input {
    flex: none;
    margin-top: 3px;
  }
  .dflt {
    margin-left: 8px;
  }
  small {
    display: block;
    margin-top: 2px;
  }
  .note {
    font-size: 12px;
    margin: 0 0 10px;
  }
</style>
