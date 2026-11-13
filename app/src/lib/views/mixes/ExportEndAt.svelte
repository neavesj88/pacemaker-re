<script lang="ts">
  // Shared by the export dialog and the Settings view: the same state (`mixExport.prefs`, kept on
  // this computer), so a change in one place is the change in the other.
  import { mixExport as X } from "$lib/state/mixexport.svelte";

  X.ensureLoaded();

  function changed() {
    X.save();
  }
</script>

<div class="row">
<label for="xr-end">The range ends</label>
<select id="xr-end" bind:value={X.prefs.endAt} onchange={changed}>
  <option value="clipEnd">when the last track ends (the next track is cut off)</option>
  <option value="nextEnters">just before the next track comes in</option>
</select>
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
</style>
