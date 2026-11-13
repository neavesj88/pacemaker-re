<script lang="ts">
  // Tags and colour for the given cases or mixes (one or many). Choices are queued, listed in
  // words with exact counts, and written together by Save. Labels belong to this app, not to the
  // device: saving does not touch the Pacemaker's library. Same fields as the track editor; a
  // case or mix has no energy.
  import { untrack } from "svelte";
  import type { ThingKind } from "$lib/api/tags";
  import { cases } from "$lib/state/cases.svelte";
  import { NO_LABELS, draftChangesAnything, draftOps, emptyDraft, thingsWord, type LabelDraft } from "$lib/state/labels";
  import { mixes } from "$lib/state/mixes.svelte";
  import { caseLabels, mixLabels, thingEditor } from "$lib/state/thingLabels.svelte";
  import { toast } from "$lib/state/toast.svelte";
  import Button from "$lib/ui/Button.svelte";
  import Modal from "$lib/ui/Modal.svelte";
  import LabelFields from "./LabelFields.svelte";

  let { kind, ids }: { kind: ThingKind; ids: number[] } = $props();

  // the editor is keyed on its ids by Overlays, so they are fixed for this instance
  const startIds = untrack(() => [...ids]);
  const store = untrack(() => (kind === "case" ? caseLabels : mixLabels));
  const noun = untrack(() => kind);
  let draft = $state<LabelDraft>(emptyDraft());
  let busy = $state(false);
  let confirmDiscard = $state(false);

  const nameOf = (id: number): string => {
    const found = kind === "case" ? cases.infos.find((c) => c.id === id)?.name : mixes.list.find((m) => m.id === id)?.name;
    return found || `${noun[0].toUpperCase()}${noun.slice(1)} ${id}`;
  };
  const targets = $derived(startIds.map((id) => store.get(id) ?? NO_LABELS));
  const changed = $derived(draftChangesAnything(draft, targets));
  const touched = $derived(draftOps(draft).length > 0);

  function requestClose() {
    if (busy) return;
    if (touched) confirmDiscard = true;
    else thingEditor.close();
  }

  async function save() {
    if (busy) return;
    if (!changed) {
      thingEditor.close();
      return;
    }
    busy = true;
    const ok = await store.apply(startIds, draftOps(draft));
    busy = false;
    if (!ok) return; // the store already said why; the dialog stays so nothing is lost
    toast.show(startIds.length === 1 ? `Saved the labels of 1 ${noun}` : `Saved the labels of ${startIds.length} ${thingsWord(noun)}`, "ok");
    thingEditor.close();
  }
</script>

<Modal title={startIds.length === 1 ? `Tags and colour of this ${noun}` : `Tags and colour of ${startIds.length} ${thingsWord(noun)}`} onclose={requestClose}>
  <p class="sub">These labels are kept by this app on this computer. They do not appear on the Pacemaker.</p>
  {#if startIds.length === 1}
    <p class="one" title={nameOf(startIds[0])}>{nameOf(startIds[0])}</p>
  {:else}
    <details class="list" open={startIds.length <= 6}>
      <summary>Applies to {startIds.length} {thingsWord(noun)}</summary>
      <ul>
        {#each startIds as id (id)}<li>{nameOf(id)}</li>{/each}
      </ul>
    </details>
  {/if}

  <LabelFields ids={startIds} bind:draft disabled={busy} get={(id) => store.get(id)} vocabulary={store.vocabulary} thing={noun} showEnergy={false} />

  {#if confirmDiscard}
    <div class="warn" role="alertdialog" aria-label="Discard changes">
      <p>Discard the unsaved changes to {startIds.length === 1 ? `this ${noun}` : `these ${startIds.length} ${thingsWord(noun)}`}?</p>
      <Button data-autofocus onclick={() => (confirmDiscard = false)}>Keep editing</Button>
      <Button onclick={() => thingEditor.close()}>Discard changes</Button>
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
