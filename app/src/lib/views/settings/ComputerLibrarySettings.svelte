<script lang="ts">
  import { session } from "$lib/state/session.svelte";
  import { toast } from "$lib/state/toast.svelte";
  import Button from "$lib/ui/Button.svelte";

  // The two choices for the library on this computer: where it is and whether it opens when the
  // app starts. Shown in the Settings view and in the Device view, so a change in one is the change
  // in the other. A change is saved at once. Changing the folder never moves or deletes the library
  // at the old one.
  const c = $derived(session.computer);
  let busy = $state(false);

  $effect(() => {
    if (!session.computer) void session.refreshComputer();
  });

  async function change() {
    busy = true;
    const old = c?.path;
    if (await session.chooseComputerFolder()) {
      toast.show(`The library on this computer is now ${session.computer?.path}. The one in ${old} was not moved or deleted.`, "ok", 8000);
    }
    busy = false;
  }

  async function useDefault() {
    if (!c) return;
    busy = true;
    await session.saveComputer({ path: "", open_at_start: c.settings.open_at_start });
    busy = false;
  }

  async function setOpenAtStart(on: boolean) {
    if (!c) return;
    busy = true;
    await session.saveComputer({ path: c.settings.path, open_at_start: on });
    busy = false;
  }

  async function makeNow() {
    busy = true;
    await session.createComputer();
    busy = false;
  }
</script>

<div class="box">
  {#if !c}
    <p class="muted">Reading the settings…</p>
  {:else}
    {#if c.warning}<p class="warn" role="status">{c.warning}</p>{/if}
    <div class="row">
      <span class="k">Folder</span>
      <code class="path">{c.path}</code>
      <Button small disabled={busy} onclick={change}>Change…</Button>
      {#if c.settings.path !== ""}<Button small variant="link" disabled={busy} onclick={useDefault}>Use the default</Button>{/if}
    </div>
    <p class="muted note">
      {#if c.exists}A library is there.{:else}No library is there yet.{/if}
      The default is the Music folder, in a folder called Pacemaker Revival. Changing the folder does not move or delete the
      library in the old one; copy it yourself if you want it in the new place.
      {#if !c.exists}<Button small variant="link" disabled={busy} onclick={makeNow}>Make it now</Button>{/if}
    </p>
    <label class="check">
      <input type="checkbox" checked={c.settings.open_at_start} disabled={busy} onchange={(e) => setOpenAtStart(e.currentTarget.checked)} />
      Open this library when the app starts
      <span class="muted">(a Pacemaker is never opened by itself: you choose when it is read)</span>
    </label>
  {/if}
</div>

<style>
  .box {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }
  .k {
    color: var(--muted);
    min-width: 50px;
  }
  .path {
    overflow-wrap: anywhere;
  }
  .note {
    margin: 0;
  }
  .warn {
    margin: 0;
    color: var(--bad-text);
  }
  .check {
    display: flex;
    align-items: baseline;
    gap: 6px;
  }
</style>
