<script lang="ts">
  import type { Snippet } from "svelte";
  import Button from "$lib/ui/Button.svelte";
  import Modal from "$lib/ui/Modal.svelte";

  // A confirmation for something destructive. It names every affected item in words (`items`),
  // and Cancel has the focus so Enter never confirms by accident.
  let {
    title,
    message,
    items = [],
    confirmLabel,
    onconfirm,
    onclose,
    extra,
  }: {
    title: string;
    message: string;
    items?: string[];
    confirmLabel: string;
    onconfirm: () => void;
    onclose: () => void;
    /** Optional extra controls (a checkbox) between the list and the buttons. */
    extra?: Snippet;
  } = $props();
</script>

<Modal {title} {onclose}>
  <p>{message}</p>
  {#if items.length > 0}
    <ul>
      {#each items as item, i (i)}<li>{item}</li>{/each}
    </ul>
  {/if}
  {@render extra?.()}
  {#snippet actions()}
    <Button data-autofocus onclick={onclose}>Cancel</Button>
    <Button
      onclick={() => {
        onconfirm();
        onclose();
      }}
    >{confirmLabel}</Button>
  {/snippet}
</Modal>

<style>
  p {
    margin: 0 0 8px;
  }
  ul {
    margin: 0 0 8px;
    padding: 6px 6px 6px 22px;
    max-height: 220px;
    overflow: auto;
    background: var(--deep);
    border: 1px solid var(--line);
    border-radius: 4px;
  }
</style>
