<script lang="ts">
  import { mixes } from "$lib/state/mixes.svelte";
  import { toast } from "$lib/state/toast.svelte";
  import Button from "$lib/ui/Button.svelte";
  import Modal from "$lib/ui/Modal.svelte";
  import { errText } from "./actions";
  import { unsaved } from "./confirm.svelte";
  import { saveMix } from "./flow";

  let busy = $state(false);

  async function saveThen() {
    const next = unsaved.next;
    if (!next) return;
    busy = true;
    try {
      await saveMix();
      unsaved.next = null;
      await next();
    } catch (e) {
      // keep the dialog closed but the mix open: the message names what blocks the save
      unsaved.next = null;
      toast.show(`Not saved: ${errText(e)}`, "bad", 9000);
    } finally {
      busy = false;
    }
  }

  async function discard() {
    const next = unsaved.next;
    unsaved.next = null;
    if (next) await next();
  }
</script>

{#if unsaved.next && mixes.doc}
  <Modal title="Unsaved changes" onclose={() => (unsaved.next = null)}>
    <p>
      “{mixes.doc.name}” has {mixes.undoDepth} unsaved edit{mixes.undoDepth === 1 ? "" : "s"}. Save them before
      continuing? Rating, genre and comments are written to the library as you change them, so Discard does not
      undo those.
    </p>
    {#snippet actions()}
      <Button onclick={() => (unsaved.next = null)} data-autofocus>Cancel</Button>
      <Button onclick={discard}><span class="bad">Discard changes</span></Button>
      <Button variant="primary" disabled={busy} onclick={saveThen}>Save</Button>
    {/snippet}
  </Modal>
{/if}

<style>
  p {
    margin: 0;
  }
  .bad {
    color: var(--bad);
  }
</style>
