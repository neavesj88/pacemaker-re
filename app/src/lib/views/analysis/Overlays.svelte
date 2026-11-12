<script lang="ts">
  // Always-mounted dialogs of the key tools (opened by `analysis.askDetectKeys` / `askSetKey`).
  import { analysis } from "$lib/state/analysis.svelte";
  import KeyDetectDialog from "./KeyDetectDialog.svelte";
  import KeyPickerDialog from "./KeyPickerDialog.svelte";

  const d = $derived(analysis.dialog);
</script>

{#if d?.kind === "detect"}
  <KeyDetectDialog ids={d.ids} />
{:else if d?.kind === "set"}
  <KeyPickerDialog ids={d.ids} />
{/if}
