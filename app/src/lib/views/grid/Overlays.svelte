<script lang="ts">
  // Always-mounted dialog of the tempo tools (opened by `tempo.askSet`).
  import { tempo } from "$lib/state/tempo.svelte";
  import BpmDialog from "./BpmDialog.svelte";
</script>

{#if tempo.settingId !== null}
  <BpmDialog trackId={tempo.settingId} />
{/if}
