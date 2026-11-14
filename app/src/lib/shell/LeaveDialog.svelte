<script lang="ts">
  import "./leaveGuards";
  import { unsavedWork } from "$lib/state/leaveGuard";
  import { session } from "$lib/state/session.svelte";
  import Button from "$lib/ui/Button.svelte";
  import Modal from "$lib/ui/Modal.svelte";

  // Opening another library closes the open one and drops what was edited in memory. This asks
  // first, naming every unsaved thing; Cancel keeps the library and the edits exactly as they were.
  let busy = $state(false);
  let failed = $state("");

  const leaving = $derived(session.leaving);
  const lines = $derived(leaving ? leaving.work.flatMap((w) => w.lines) : []);

  function cancel() {
    failed = "";
    leaving?.done(false);
  }

  function discard() {
    failed = "";
    leaving?.done(true);
  }

  async function saveThenGo() {
    const l = leaving;
    if (!l) return;
    busy = true;
    failed = "";
    try {
      for (const w of l.work) {
        if (!(await w.save?.())) {
          failed = "Not everything could be saved, so nothing was closed. Check the message in the editor, or discard the changes.";
          return;
        }
      }
      l.done(true);
    } catch (e) {
      failed = `Not saved: ${e instanceof Error ? e.message : String(e)}`;
    } finally {
      busy = false;
    }
  }

  // closing the window with unsaved work in memory
  function onbeforeunload(e: BeforeUnloadEvent) {
    if (unsavedWork().length > 0) e.preventDefault();
  }
</script>

<svelte:window {onbeforeunload} />

{#if leaving}
  <Modal title="Unsaved changes" onclose={cancel}>
    <p>Opening another library closes this one, and these changes are lost:</p>
    <ul>
      {#each lines as l, i (i)}<li>{l}</li>{/each}
    </ul>
    {#if failed}<p class="bad" role="alert">{failed}</p>{/if}
    {#snippet actions()}
      <Button onclick={cancel} data-autofocus>Cancel</Button>
      <Button onclick={discard} disabled={busy}><span class="bad">Discard and open the other library</span></Button>
      {#if session.leavingSaveable}
        <Button variant="primary" disabled={busy} onclick={saveThenGo}>Save, then open it</Button>
      {/if}
    {/snippet}
  </Modal>
{/if}

<style>
  p {
    margin: 0 0 8px;
  }
  ul {
    margin: 0 0 8px;
    padding-left: 20px;
  }
  .bad {
    color: var(--bad);
  }
</style>
