<script lang="ts">
  // Shared by the export dialog and the Settings view: the same state (`mixExport.prefs`, kept on
  // this computer), so a change in one place is the change in the other.
  import { mixExport as X } from "$lib/state/mixexport.svelte";
  import { TARGET_PRESETS } from "$lib/state/exportprefs";

  X.ensureLoaded();

  function changed() {
    X.save();
  }
</script>

<div class="row">
  <label for="xl-target">Target loudness</label>
  <select id="xl-target" bind:value={X.prefs.targetPreset} onchange={changed}>
    {#each TARGET_PRESETS as t (t.id)}<option value={t.id} title={t.hint}>{t.label}</option>{/each}
  </select>
  {#if X.prefs.targetPreset === "custom"}
    <input class="num-in" type="number" step="0.5" bind:value={X.prefs.customTarget} onchange={changed} aria-label="Target loudness in LUFS" />
    <span class="muted">LUFS</span>
  {/if}
</div>
<div class="row">
  <label for="xl-clip">Level the tracks</label>
  <select id="xl-clip" bind:value={X.prefs.clipLevel} onchange={changed}>
    <option value="off">no, keep each track as it is</option>
    <option value="median">to the median loudness of the tracks</option>
    <option value="fixed">to a fixed loudness</option>
  </select>
  {#if X.prefs.clipLevel === "fixed"}
    <input class="num-in" type="number" step="0.5" bind:value={X.prefs.clipFixedLufs} onchange={changed} aria-label="Loudness every track is moved to, in LUFS" />
    <span class="muted">LUFS</span>
  {/if}
</div>

<style>
  .row {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 8px;
    margin: 6px 0;
  }
  .row label {
    color: var(--muted);
    font-size: 12px;
  }
  .num-in {
    width: 78px;
  }
</style>
