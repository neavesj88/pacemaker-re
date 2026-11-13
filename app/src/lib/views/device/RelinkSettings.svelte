<script lang="ts">
  // The missing-audio search options for the Settings view: the search dialog's own option fields,
  // plus the Save they do not need there (the dialog saves them when a search starts).
  import Button from "$lib/ui/Button.svelte";
  import { relink } from "./relink.svelte";
  import RelinkOptionsFields from "./RelinkOptionsFields.svelte";
</script>

{#if relink.settingsWarning}<p class="warn">{relink.settingsWarning}</p>{/if}
<RelinkOptionsFields />
<div class="actions">
  <Button variant="primary" disabled={!relink.settings} onclick={() => void relink.saveSettings()}>Save</Button>
  <span class="muted">Saved on this computer, not on the Pacemaker. The search dialog saves them too when a search starts.</span>
</div>

<style>
  .actions {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-top: 8px;
  }
  .warn {
    color: var(--cue);
  }
</style>
