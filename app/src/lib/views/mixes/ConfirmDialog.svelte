<script lang="ts">
  import Button from "$lib/ui/Button.svelte";
  import Modal from "$lib/ui/Modal.svelte";
  import { toast } from "$lib/state/toast.svelte";
  import { errText } from "./actions";
  import { confirm } from "./confirm.svelte";

  let busy = $state(false);

  async function go() {
    const r = confirm.req;
    if (!r || busy) return;
    busy = true;
    try {
      await r.run();
      confirm.close();
    } catch (e) {
      toast.show(errText(e), "bad", 8000);
      confirm.close();
    } finally {
      busy = false;
    }
  }
</script>

{#if confirm.req}
  {@const r = confirm.req}
  <Modal title={r.title} onclose={() => confirm.close()}>
    <p class="msg">{r.message}</p>
    {#if r.items?.length}
      <ul class="items">
        {#each r.items.slice(0, 40) as it, i (i)}<li>{it}</li>{/each}
        {#if r.items.length > 40}<li class="muted">and {r.items.length - 40} more</li>{/if}
      </ul>
    {/if}
    {#snippet actions()}
      <Button onclick={() => confirm.close()} data-autofocus>Cancel</Button>
      <Button variant={r.danger ? "default" : "primary"} disabled={busy} onclick={go}>
        <span class:bad={r.danger}>{r.confirmLabel}</span>
      </Button>
    {/snippet}
  </Modal>
{/if}

<style>
  .msg {
    margin: 0 0 8px;
  }
  .items {
    margin: 0;
    padding-left: 18px;
    max-height: 220px;
    overflow: auto;
    font-size: 13px;
  }
  .muted {
    color: var(--muted);
  }
  .bad {
    color: var(--bad);
  }
</style>
