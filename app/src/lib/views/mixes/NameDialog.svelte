<script lang="ts">
  // Asks for a mix name (new mix, rename). The backend's refusal (taken, empty, too long) is shown
  // inside the dialog so the name can be fixed without starting over.
  import Button from "$lib/ui/Button.svelte";
  import Modal from "$lib/ui/Modal.svelte";
  import { errText } from "./actions";
  import { namer } from "./confirm.svelte";

  let value = $state("");
  let error = $state("");
  let busy = $state(false);
  let seen: unknown = null;

  $effect(() => {
    const r = namer.req;
    if (r !== seen) {
      seen = r;
      value = r?.initial ?? "";
      error = "";
    }
  });

  async function submit() {
    const r = namer.req;
    const name = value.trim();
    if (!r || busy) return;
    if (!name) {
      error = "Enter a name.";
      return;
    }
    if (name.length > 255) {
      error = "A mix name can be at most 255 characters.";
      return;
    }
    busy = true;
    try {
      await r.run(name);
      namer.req = null;
    } catch (e) {
      error = errText(e);
    } finally {
      busy = false;
    }
  }
</script>

{#if namer.req}
  {@const r = namer.req}
  <Modal title={r.title} onclose={() => (namer.req = null)}>
    <form
      onsubmit={(e) => {
        e.preventDefault();
        void submit();
      }}
    >
      <label for="mix-name">{r.label}</label>
      <!-- svelte-ignore a11y_autofocus -->
      <input id="mix-name" type="text" maxlength="255" bind:value autofocus data-autofocus autocomplete="off" />
      {#if error}<div class="err" role="alert">{error}</div>{/if}
    </form>
    {#snippet actions()}
      <Button onclick={() => (namer.req = null)}>Cancel</Button>
      <Button variant="primary" disabled={busy} onclick={submit}>{r.confirmLabel}</Button>
    {/snippet}
  </Modal>
{/if}

<style>
  form {
    display: grid;
    gap: 6px;
  }
  label {
    color: var(--muted);
    font-size: 12px;
  }
  input {
    width: 100%;
    box-sizing: border-box;
  }
  .err {
    background: var(--bad-bg);
    color: var(--bad-text);
    border-radius: 4px;
    padding: 6px 8px;
    font-size: 12px;
  }
</style>
