<script lang="ts">
  // Tags, colour and energy for the given tracks (one or many). Choices are queued, listed in
  // words with exact counts, and written together by Save. Labels belong to this app, not to the
  // device: saving does not touch the Pacemaker's library.
  import { untrack } from "svelte";
  import { NO_LABELS, draftChangesAnything, draftOps, emptyDraft, type LabelDraft } from "$lib/state/labels";
  import { labelEditor, labels } from "$lib/state/labels.svelte";
  import { session } from "$lib/state/session.svelte";
  import { toast } from "$lib/state/toast.svelte";
  import Button from "$lib/ui/Button.svelte";
  import Modal from "$lib/ui/Modal.svelte";
  import LabelFields from "./LabelFields.svelte";

  let { ids }: { ids: number[] } = $props();

  // the editor is keyed on its ids by Overlays, so they are fixed for this instance
  const startIds = untrack(() => [...ids]);
  let draft = $state<LabelDraft>(emptyDraft());
  let busy = $state(false);
  let confirmDiscard = $state(false);

  const byId = $derived(new Map((session.library?.tracks ?? []).map((t) => [t.track_id, t])));
  const name = (id: number) => {
    const t = byId.get(id);
    return t ? [t.title, t.artist].filter(Boolean).join(" — ") || `Track ${id}` : `Track ${id}`;
  };
  const targets = $derived(startIds.map((id) => labels.get(id) ?? NO_LABELS));
  const changed = $derived(draftChangesAnything(draft, targets));
  const touched = $derived(draftOps(draft).length > 0);

  function requestClose() {
    // the tag manager can be open over this dialog and owns Escape while it is
    if (busy || labelEditor.managing) return;
    if (touched) confirmDiscard = true;
    else labelEditor.closeEditor();
  }

  async function save() {
    if (busy) return;
    if (!changed) {
      labelEditor.closeEditor();
      return;
    }
    busy = true;
    const ok = await labels.apply(startIds, draftOps(draft));
    busy = false;
    if (!ok) return; // the store already said why; the dialog stays so nothing is lost
    toast.show(startIds.length === 1 ? "Saved the labels of 1 track" : `Saved the labels of ${startIds.length} tracks`, "ok");
    labelEditor.closeEditor();
  }
</script>

<Modal title={startIds.length === 1 ? "Tags, colour and energy" : `Tags, colour and energy of ${startIds.length} tracks`} onclose={requestClose}>
  <p class="sub">
    These labels are kept by this app on this computer. They do not appear on the Pacemaker.
  </p>
  {#if startIds.length === 1}
    <p class="one" title={name(startIds[0])}>{name(startIds[0])}</p>
  {:else}
    <details class="list">
      <summary>Applies to {startIds.length} tracks</summary>
      <ul>
        {#each startIds as id (id)}<li>{name(id)}</li>{/each}
      </ul>
    </details>
  {/if}

  <LabelFields ids={startIds} bind:draft disabled={busy} />

  <div class="more">
    <Button variant="link" onclick={() => labelEditor.manage()}>Manage tags…</Button>
    <span class="muted">rename, merge or delete a tag across the library</span>
  </div>

  {#if confirmDiscard}
    <div class="warn" role="alertdialog" aria-label="Discard changes">
      <p>Discard the unsaved changes to {startIds.length === 1 ? "this track" : `these ${startIds.length} tracks`}?</p>
      <Button data-autofocus onclick={() => (confirmDiscard = false)}>Keep editing</Button>
      <Button onclick={() => labelEditor.closeEditor()}>Discard changes</Button>
    </div>
  {/if}

  {#snippet actions()}
    <Button onclick={requestClose} disabled={busy}>Cancel</Button>
    <Button variant="primary" onclick={save} disabled={busy || !changed}>{busy ? "Saving…" : "Save"}</Button>
  {/snippet}
</Modal>

<style>
  .sub {
    margin: 0 0 8px;
    color: var(--muted);
    font-size: 12px;
  }
  .one {
    margin: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .list summary {
    cursor: pointer;
  }
  .list ul {
    max-height: 120px;
    overflow: auto;
    margin: 4px 0 0;
    padding: 0 0 0 18px;
    font-size: 12px;
  }
  .more {
    display: flex;
    gap: 8px;
    align-items: baseline;
    margin-top: 10px;
    font-size: 12px;
  }
  .warn {
    margin-top: 12px;
    padding: 6px 10px;
    background: var(--bad-bg);
    color: var(--bad-text);
    border-radius: 4px;
  }
  .warn p {
    margin: 0 0 8px;
  }
</style>
