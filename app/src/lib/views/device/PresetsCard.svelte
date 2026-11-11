<script lang="ts">
  import {
    devicePresetDelete,
    devicePresetExport,
    devicePresetReadFile,
    devicePresetsList,
    pickPresetFile,
    pickPresetSavePath,
    type Preset,
    type PresetList,
    type PresetSummary,
    type SaveResult,
  } from "$lib/api/device";
  import { session } from "$lib/state/session.svelte";
  import { toast } from "$lib/state/toast.svelte";
  import Button from "$lib/ui/Button.svelte";
  import Modal from "$lib/ui/Modal.svelte";
  import Card from "./Card.svelte";
  import { device } from "./device.svelte";
  import PresetApplyModal from "./PresetApplyModal.svelte";
  import PresetNameModal from "./PresetNameModal.svelte";
  import { presetMeta } from "./presetLogic";
  import { stampLabel } from "./trustLogic";

  // Named copies of the settings above. They live in this app's settings folder on the PC (the
  // Pacemaker has nowhere to keep them), can be exported to a .json file and imported on another
  // PC, and are applied to the card that is open after a list of every change.
  let list = $state<PresetList | null>(null);
  let error = $state("");
  let saving = $state(false);
  let importing = $state<{ path: string; file: Preset } | null>(null);
  let applying = $state<string | null>(null);
  let deleting = $state<PresetSummary | null>(null);
  let deletingNow = $state(false);

  async function reload() {
    try {
      list = await devicePresetsList();
      error = "";
    } catch (e) {
      error = String(e);
    }
  }

  $effect(() => {
    void reload();
  });

  const lib = $derived(session.library);
  const canUseCard = $derived(!!lib && device.settings != null);
  const noCardWhy = $derived(device.settings === null ? "This card has no settings file yet" : undefined);

  async function pickImport() {
    try {
      const path = await pickPresetFile();
      if (!path) return;
      importing = { path, file: await devicePresetReadFile(path) };
    } catch (e) {
      toast.show(String(e), "bad", 8000);
    }
  }

  async function exportOne(p: PresetSummary) {
    try {
      const dest = await pickPresetSavePath(p.name);
      if (!dest) return;
      await devicePresetExport(p.name, dest);
      toast.show(`"${p.name}" saved to ${dest}`, "ok", 6000);
    } catch (e) {
      toast.show(String(e), "bad", 8000);
    }
  }

  async function removeOne() {
    const p = deleting;
    if (!p) return;
    deletingNow = true;
    try {
      await devicePresetDelete(p.name);
      toast.show(`Preset "${p.name}" deleted. The card was not changed.`, "ok", 5000);
      deleting = null;
      await reload();
    } catch (e) {
      toast.show(String(e), "bad", 8000);
    } finally {
      deletingNow = false;
    }
  }

  async function stored(r: SaveResult) {
    saving = false;
    importing = null;
    if (r.summary) toast.show(`Preset "${r.summary.name}" stored (${r.summary.settings} settings).`, "ok", 5000);
    await reload();
  }

  const when = (p: PresetSummary) => (p.saved_utc ? stampLabel(p.saved_utc) : "");
</script>

<Card title="Settings presets">
  <p class="muted">
    A preset is a named copy of the values in Device settings above. It is kept on this PC, not on the Pacemaker (the
    device has nowhere to store it), so you can export it, import it on another PC, and apply it to a different card.
    Applying shows every change first.
  </p>
  {#if error}
    <p class="bad">{error}</p>
  {:else if !list}
    <p class="muted">Reading the presets…</p>
  {:else if list.presets.length === 0}
    <p class="muted">No presets yet. Save this card's settings, or add a preset file.</p>
  {:else}
    <ul class="rows" aria-label="Presets">
      {#each list.presets as p (p.file)}
        <li>
          <div class="what">
            <b>{p.name}</b>
            <div class="meta muted">{presetMeta(p, when(p))}</div>
          </div>
          <div class="btns">
            <Button
              small
              variant="primary"
              disabled={!canUseCard || device.blocked}
              title={noCardWhy ?? (device.blocked ? "A job is still writing to the device" : undefined)}
              onclick={() => (applying = p.name)}>Apply…</Button
            >
            <Button small onclick={() => exportOne(p)}>Export…</Button>
            <Button small variant="ghost" onclick={() => (deleting = p)}>Delete…</Button>
          </div>
        </li>
      {/each}
    </ul>
  {/if}
  {#if list && list.unreadable.length}
    <details class="unreadable">
      <summary>{list.unreadable.length === 1 ? "1 file in the presets folder" : `${list.unreadable.length} files in the presets folder`} could not be read</summary>
      <ul>
        {#each list.unreadable as u (u)}<li>{u}</li>{/each}
      </ul>
      <p class="muted">They are left as they are; delete or fix them in the folder.</p>
    </details>
  {/if}
  <div class="actions">
    <Button disabled={!canUseCard} title={noCardWhy} onclick={() => (saving = true)}>Save this card's settings…</Button>
    <Button onclick={pickImport}>Add from file…</Button>
  </div>
</Card>

{#if saving}
  <PresetNameModal mode="save" existing={list?.presets ?? []} onclose={() => (saving = false)} ondone={stored} />
{/if}
{#if importing}
  <PresetNameModal
    mode="import"
    existing={list?.presets ?? []}
    file={importing.file}
    path={importing.path}
    onclose={() => (importing = null)}
    ondone={stored}
  />
{/if}
{#if applying}
  <PresetApplyModal name={applying} onclose={() => (applying = null)} />
{/if}
{#if deleting}
  <Modal title={`Delete the preset "${deleting.name}"?`} onclose={() => (deleting = null)}>
    <p>
      {presetMeta(deleting, deleting.saved_utc ? stampLabel(deleting.saved_utc) : "")}. Its file in this app's settings
      folder is removed and cannot be brought back. The Pacemaker is not changed, and nothing already applied is undone.
    </p>
    {#snippet actions()}
      <Button data-autofocus onclick={() => (deleting = null)}>Keep it</Button>
      <Button variant="primary" disabled={deletingNow} onclick={removeOne}>Delete "{deleting?.name}"</Button>
    {/snippet}
  </Modal>
{/if}

<style>
  .rows {
    list-style: none;
    margin: 8px 0 0;
    padding: 0;
    border: 1px solid var(--line);
    border-radius: 4px;
  }
  .rows > li {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    padding: 6px 8px;
  }
  .rows > li:nth-child(odd) {
    background: var(--row);
  }
  .what {
    min-width: 0;
    overflow-wrap: anywhere;
  }
  .meta {
    font-size: 12px;
  }
  .btns {
    display: flex;
    gap: 6px;
    flex-shrink: 0;
  }
  .actions {
    display: flex;
    gap: 8px;
    margin-top: 10px;
    flex-wrap: wrap;
  }
  .bad {
    color: var(--bad-text);
  }
  .unreadable {
    margin-top: 8px;
    font-size: 12px;
    color: var(--bad-text);
  }
  .unreadable ul {
    margin: 4px 0;
    padding-left: 18px;
    overflow-wrap: anywhere;
  }
</style>
