<script lang="ts">
  // Always-mounted overlay of the metadata area (picked up by shell/overlays.ts): the track
  // metadata editor, opened with metadataEditor.edit(ids); the tag, colour and energy editor,
  // opened with labelEditor.edit(ids); the tag manager, opened with labelEditor.manage(); the tag
  // and colour editor of cases and mixes, opened with thingEditor.edit(kind, ids). It also reads
  // the labels of whichever library is open (tracks, cases, mixes) and the Energy source setting,
  // so every view can show them.
  import { untrack } from "svelte";
  import { cases } from "$lib/state/cases.svelte";
  import { energy } from "$lib/state/energy.svelte";
  import { energySource } from "$lib/state/energySource.svelte";
  import { labelEditor, labels } from "$lib/state/labels.svelte";
  import { metadataEditor } from "$lib/state/metadata.svelte";
  import { mixes } from "$lib/state/mixes.svelte";
  import { session } from "$lib/state/session.svelte";
  import { caseLabels, mixLabels, thingEditor } from "$lib/state/thingLabels.svelte";
  import MetadataEditor from "./MetadataEditor.svelte";
  import TagEditor from "./TagEditor.svelte";
  import TagManager from "./TagManager.svelte";
  import ThingLabelEditor from "./ThingLabelEditor.svelte";

  // the labels belong to the open library; read them when it (or a different one) opens
  $effect(() => {
    const root = session.library?.root;
    if (root && labels.root !== root) void labels.load(root);
  });

  // a case or mix is labelled by id and creation time, so they are read again whenever the list
  // of cases (or mixes) changes: a deleted case's id can come back on a new one
  $effect(() => {
    const root = session.library?.root;
    void cases.infos;
    if (root) void caseLabels.load(root);
  });
  $effect(() => {
    const root = session.library?.root;
    void mixes.list;
    if (root) void mixLabels.load(root);
  });

  // smart-case counts in the sidebar depend on labels and on the energy choice
  $effect(() => {
    void labels.version;
    void energySource.source;
    void energy.state;
    untrack(() => cases.scheduleSmartCounts());
  });

  // the energy choice is the app's own, not per library: read once
  $effect(() => {
    if (!energySource.loaded) void energySource.load();
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

{#if thingEditor.ids}
  {#key thingEditor.ids}
    <ThingLabelEditor kind={thingEditor.kind} ids={thingEditor.ids} />
  {/key}
{/if}
