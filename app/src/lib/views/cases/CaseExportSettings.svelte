<script lang="ts">
  // The starting choices of the case export dialog, for the Settings view: the dialog's own option
  // fields on a copy of the saved choices, with the Save the dialog does not need (it saves them
  // itself when an export starts). The folder is remembered by the dialog, not set here.
  import { caseExport as ex } from "$lib/state/caseExport.svelte";
  import { EXT, FORMAT_LABEL, startingPrefs } from "$lib/state/exportLogic";
  import Button from "$lib/ui/Button.svelte";
  import ExportOptionsFields from "./ExportOptionsFields.svelte";

  const FORMATS = ["m3u8", "csv", "rekordbox"] as const;
  const uid = $props.id();

  let draft = $state(startingPrefs(ex.saved));
  // the saved values replace the form when they are loaded or saved, never while changing it
  $effect(() => {
    draft = startingPrefs(ex.saved);
  });

  const changed = $derived(JSON.stringify(draft) !== JSON.stringify(startingPrefs(ex.saved)));
</script>

{#if ex.warning}<p class="warn" role="alert">{ex.warning}</p>{/if}
<div class="row">
  <span class="lbl">Format</span>
  {#each FORMATS as f (f)}
    <label class="inline"><input type="radio" name="{uid}-fmt" value={f} bind:group={draft.format} /> {FORMAT_LABEL[f]} <span class="muted">.{EXT[f]}</span></label>
  {/each}
</div>
<ExportOptionsFields bind:prefs={draft} canReplace={false} />
<div class="actions">
  <Button variant="primary" disabled={!changed} onclick={() => void ex.saveChoices($state.snapshot(draft))}>Save</Button>
  <Button variant="ghost" disabled={!changed} onclick={() => (draft = startingPrefs(ex.saved))}>Revert</Button>
  <span class="muted">Saved on this computer, not on the Pacemaker. Replacing a file is chosen for one export, never saved.</span>
</div>

<style>
  .row {
    display: flex;
    align-items: baseline;
    flex-wrap: wrap;
    gap: 8px;
    margin: 6px 0;
  }
  .lbl {
    color: var(--muted);
    font-size: 12px;
  }
  .inline {
    display: inline-flex;
    gap: 5px;
    align-items: baseline;
    margin-right: 12px;
    cursor: pointer;
  }
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
