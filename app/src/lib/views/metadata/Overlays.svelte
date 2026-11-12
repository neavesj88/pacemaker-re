<script lang="ts">
  // Always-mounted overlay of the metadata area (picked up by shell/overlays.ts): the track
  // metadata editor, opened with metadataEditor.edit(ids); the tag, colour and energy editor,
  // opened with labelEditor.edit(ids); the tag manager, opened with labelEditor.manage(). It also
  // reads the labels of whichever library is open, so every view can show them.
  import { labelEditor, labels } from "$lib/state/labels.svelte";
  import { metadataEditor } from "$lib/state/metadata.svelte";
  import { session } from "$lib/state/session.svelte";
  import MetadataEditor from "./MetadataEditor.svelte";
  import TagEditor from "./TagEditor.svelte";
  import TagManager from "./TagManager.svelte";

  // the labels belong to the open library; read them when it (or a different one) opens
  $effect(() => {
    const root = session.library?.root;
    if (root && labels.root !== root) void labels.load(root);
  });
</script>

{#if metadataEditor.ids}
  {#key metadataEditor.ids}
    <MetadataEditor ids={metadataEditor.ids} />
  {/key}
{/if}

{#if labelEditor.ids}
  {#key labelEditor.ids}
    <TagEditor ids={labelEditor.ids} />
  {/key}
{/if}

{#if labelEditor.managing}
  <TagManager />
{/if}
