<script lang="ts">
  // Always-mounted overlay of the metadata area (picked up by shell/overlays.ts): the track
  // metadata editor, opened with metadataEditor.edit(ids).
  import { metadataEditor } from "$lib/state/metadata.svelte";
  import MetadataEditor from "./MetadataEditor.svelte";
</script>

{#if metadataEditor.ids}
  {#key metadataEditor.ids}
    <MetadataEditor ids={metadataEditor.ids} />
  {/key}
{/if}
