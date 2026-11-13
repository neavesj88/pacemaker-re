<script lang="ts">
  // Always-mounted overlay of the setlist feature (picked up by shell/overlays.ts): the printable
  // setlist dialog, opened with setlist.openCase(id) / setlist.openMix(id).
  import SetlistModal from "./SetlistModal.svelte";
  import { setlist } from "./setlist.svelte";
</script>

{#if setlist.request}
  {#key `${setlist.request.kind}:${setlist.request.id}`}
    <SetlistModal kind={setlist.request.kind} id={setlist.request.id} onclose={() => setlist.close()} />
  {/key}
{/if}
