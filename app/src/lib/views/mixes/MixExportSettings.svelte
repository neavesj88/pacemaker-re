<script lang="ts">
  // The mix audio export's saved choices, for the Settings view: the same field components as the
  // export dialog, on the same state, so a value set here is the value the dialog opens with.
  // Nothing about the range or the loudness measurement is here; those belong to the mix being exported.
  import { mixExport as X } from "$lib/state/mixexport.svelte";
  import ExportEndAt from "./ExportEndAt.svelte";
  import ExportFileFields from "./ExportFileFields.svelte";
  import ExportLimitFields from "./ExportLimitFields.svelte";
  import ExportLoudnessFields from "./ExportLoudnessFields.svelte";
  import ExportTagFields from "./ExportTagFields.svelte";

  X.ensureLoaded();
</script>

<fieldset>
  <legend>Loudness</legend>
  <ExportLoudnessFields />
</fieldset>
<fieldset>
  <legend>File</legend>
  <ExportFileFields />
</fieldset>
<fieldset>
  <legend>Tags</legend>
  <ExportTagFields />
  <div class="muted small">Tags are written into the exported file only; nothing is added to the device. The title is the mix's name unless you type another in the export dialog.</div>
</fieldset>
<fieldset>
  <legend>Range and limits</legend>
  <ExportEndAt />
  <label class="check">
    <input type="checkbox" bind:checked={X.prefs.previewAtExportLevel} onchange={() => X.save()} />
    Play the editor's preview at the export level (needs a current measurement)
  </label>
  <ExportLimitFields />
</fieldset>

<style>
  fieldset {
    border: 1px solid var(--line);
    border-radius: 6px;
    margin: 0 0 10px;
    padding: 8px 12px 10px;
  }
  legend {
    color: var(--muted);
    font-size: 12px;
    padding: 0 6px;
  }
  .check {
    display: block;
    margin: 6px 0;
  }
  .small {
    font-size: 12px;
  }
</style>
