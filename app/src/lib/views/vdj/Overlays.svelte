<script lang="ts">
  // Always-mounted overlay of the VirtualDJ export (picked up by shell/overlays.ts): the dialog,
  // opened with vdj.openCase(id, name).
  import VdjExportModal from "./VdjExportModal.svelte";
  import { vdj } from "./vdj.svelte";
</script>

{#if vdj.request}
  {#key vdj.request.caseId}
    <VdjExportModal caseId={vdj.request.caseId} caseName={vdj.request.name} onclose={() => vdj.close()} />
  {/key}
{/if}
