<script lang="ts">
  import { deviceCleanup, JOB_CLEANUP } from "$lib/api/device";
  import { session } from "$lib/state/session.svelte";
  import { toast } from "$lib/state/toast.svelte";
  import Button from "$lib/ui/Button.svelte";
  import Modal from "$lib/ui/Modal.svelte";
  import { device } from "./device.svelte";
  import { bytes, plural } from "./format";

  // Lists every file the scan found that no track uses, by name, and deletes only the ticked ones
  // after an explicit confirmation. Audio is permanent loss, so it says so in words.
  const files = $derived(device.health?.unreferenced ?? []);
  // beat grids of removed tracks are ticked; audio and other files are the user's call
  let unticked = $state<Set<string>>(new Set());
  let seeded = $state(false);

  $effect(() => {
    if (!seeded && files.length) {
      unticked = new Set(files.filter((f) => f.kind !== "grid").map((f) => f.path));
      seeded = true;
    }
  });

  const chosen = $derived(files.filter((f) => !unticked.has(f.path)));
  const chosenBytes = $derived(chosen.reduce((n, f) => n + f.size, 0));

  const kindText = { audio: "audio file, no track uses it", grid: "beat grid of no track", other: "other file" };

  function toggle(path: string) {
    const next = new Set(unticked);
    if (next.has(path)) next.delete(path);
    else next.add(path);
    unticked = next;
  }

  function close() {
    device.cleanupOpen = false;
    seeded = false;
  }

  async function run() {
    const lib = session.library;
    const list = chosen.map((f) => f.path);
    close();
    if (!lib || list.length === 0) return;
    device.begin(JOB_CLEANUP, "Deleting unused files");
    try {
      await deviceCleanup(lib.root, list);
    } catch (e) {
      device.job = null;
      toast.show(String(e), "bad", 8000);
    }
  }
</script>

{#if device.cleanupOpen}
  <Modal title={`Delete ${plural(chosen.length, "unused file")} from ${session.place.the}?`} onclose={close}>
    <p class="muted">
      These files are under .Pacemaker/Music on {session.library?.root} and no track in the library uses them. Ticked
      files are deleted for good; there is no recycle bin {session.isComputer ? 'for them' : 'on the device'}. Untick any you want to keep.
    </p>
    <ul class="files" aria-label="Unused files">
      {#each files as f (f.path)}
        <li>
          <label>
            <input type="checkbox" checked={!unticked.has(f.path)} onchange={() => toggle(f.path)} />
            <span class="name">{f.path}</span>
            <span class="kind muted">{kindText[f.kind]}</span>
            <span class="num muted">{bytes(f.size)}</span>
          </label>
        </li>
      {/each}
    </ul>
    {#snippet actions()}
      <Button data-autofocus onclick={close}>Cancel</Button>
      <Button variant="primary" disabled={chosen.length === 0} onclick={run}>
        Delete {plural(chosen.length, "file")} ({bytes(chosenBytes)})
      </Button>
    {/snippet}
  </Modal>
{/if}

<style>
  .files {
    list-style: none;
    margin: 8px 0 0;
    padding: 0;
    max-height: 280px;
    overflow: auto;
    border: 1px solid var(--line);
    border-radius: 4px;
  }
  li:nth-child(odd) {
    background: var(--row);
  }
  label {
    display: grid;
    grid-template-columns: auto 1fr auto;
    gap: 2px 8px;
    align-items: baseline;
    padding: 4px 8px;
    cursor: pointer;
  }
  .name {
    overflow-wrap: anywhere;
    font-family: ui-monospace, Consolas, monospace;
    font-size: 12px;
  }
  .kind {
    grid-column: 2;
    grid-row: 2;
    font-size: 11px;
  }
  .num {
    grid-column: 3;
    grid-row: 1;
    white-space: nowrap;
  }
</style>
