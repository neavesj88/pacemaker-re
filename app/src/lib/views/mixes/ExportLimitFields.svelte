<script lang="ts">
  // Shared by the export dialog and the Settings view: the same state (`mixExport.prefs`, kept on
  // this computer), so a change in one place is the change in the other.
  import { mixExport as X } from "$lib/state/mixexport.svelte";

  X.ensureLoaded();

  function changed() {
    X.save();
  }
</script>

<div class="grid">
  <label for="xs-boost">Most the master gain may raise the mix</label>
  <span><input id="xs-boost" class="num-in" type="number" step="1" min="0" max="40" bind:value={X.prefs.maxBoostDb} onchange={changed} /> dB</span>
  <label for="xs-cut">Most it may lower it</label>
  <span><input id="xs-cut" class="num-in" type="number" step="1" min="0" max="60" bind:value={X.prefs.maxCutDb} onchange={changed} /> dB</span>
  <label for="xs-clipmax">Most levelling may change one track</label>
  <span><input id="xs-clipmax" class="num-in" type="number" step="1" min="0" max="30" bind:value={X.prefs.clipMaxDb} onchange={changed} /> dB</span>
  <label for="xs-fade">Fade at a cut inside the audio (partial export)</label>
  <span><input id="xs-fade" class="num-in" type="number" step="5" min="0" max="2000" bind:value={X.prefs.edgeFadeMs} onchange={changed} /> ms (0 = hard cut)</span>
</div>
<label class="check"><input type="checkbox" bind:checked={X.prefs.capAtPeak} onchange={changed} /> Never raise the mix past a true-peak ceiling (the target may then not be reached)</label>
{#if X.prefs.capAtPeak}
  <div class="grid">
    <label for="xs-ceil">Ceiling</label>
    <span><input id="xs-ceil" class="num-in" type="number" step="0.5" min="-20" max="0" bind:value={X.prefs.peakCeilingDb} onchange={changed} /> dBTP</span>
  </div>
  <div class="muted small">The true-peak reading is accurate to about 0.3 dB, so leave that much room under the ceiling you need.</div>
{/if}

<style>
  .grid {
    display: grid;
    grid-template-columns: max-content 1fr;
    gap: 6px 12px;
    align-items: center;
    margin: 8px 0;
  }
  .grid label {
    color: var(--muted);
  }
  .num-in {
    width: 78px;
  }
  .check {
    display: block;
    margin: 6px 0;
  }
  .small {
    font-size: 12px;
  }
</style>
