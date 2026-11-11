<script lang="ts">
  import { untrack } from "svelte";
  import Button from "$lib/ui/Button.svelte";
  import Modal from "$lib/ui/Modal.svelte";

  // Asks for a case name. The device stores at most 255 characters and a name cannot be blank.
  let {
    title,
    initial = "",
    confirmLabel,
    note,
    taken = [],
    onconfirm,
    onclose,
  }: {
    title: string;
    initial?: string;
    confirmLabel: string;
    note?: string;
    /** Existing case names: a clash is warned about (the device allows it, but it is confusing). */
    taken?: string[];
    onconfirm: (name: string) => void;
    onclose: () => void;
  } = $props();

  // the dialog is mounted fresh each time, so the starting text is read once
  let name = $state(untrack(() => initial));
  let valid = $derived(name.trim().length > 0 && [...name].length <= 255);

  let clash = $derived.by(() => {
    const n = name.trim().toLowerCase();
    if (!n) return "";
    if (n === "favourites" && !taken.includes("Favourites")) return "“Favourites” is the case the heart on a track uses; a case with that name will be filled by the heart.";
    return taken.some((t) => t.trim().toLowerCase() === n) ? "A case with this name already exists. Two cases with the same name are hard to tell apart." : "";
  });

  function submit() {
    if (valid) onconfirm(name.trim());
  }
</script>

<Modal {title} {onclose}>
  <form
    onsubmit={(e) => {
      e.preventDefault();
      submit();
    }}
  >
    <!-- svelte-ignore a11y_autofocus -->
    <input data-autofocus bind:value={name} maxlength="255" aria-label="Case name" placeholder="Case name" />
    {#if note}<p class="muted">{note}</p>{/if}
    {#if clash}<p class="warn" role="status">{clash}</p>{/if}
  </form>
  {#snippet actions()}
    <Button onclick={onclose}>Cancel</Button>
    <Button variant="primary" disabled={!valid} onclick={submit}>{confirmLabel}</Button>
  {/snippet}
</Modal>

<style>
  input {
    width: 100%;
    box-sizing: border-box;
  }
  .warn {
    color: var(--bad-text);
  }
  p {
    margin: 8px 0 0;
  }
</style>
