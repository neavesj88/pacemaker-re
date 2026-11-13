<script lang="ts">
  import type { Snippet } from "svelte";

  // A dialog over a scrim. Mount it inside {#if}; it closes on Escape or a click on the scrim.
  // Focus goes to the first element marked data-autofocus (put it on the safe choice), else the dialog.
  let {
    title,
    onclose,
    children,
    actions,
  }: { title: string; onclose: () => void; children?: Snippet; actions?: Snippet } = $props();

  let dialog = $state<HTMLDivElement | null>(null);

  $effect(() => {
    const target = dialog?.querySelector<HTMLElement>("[data-autofocus]") ?? dialog;
    target?.focus();
  });
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && onclose()} />

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="scrim" onclick={onclose}>
  <div
    class="modal"
    role="dialog"
    aria-modal="true"
    aria-label={title}
    tabindex="-1"
    bind:this={dialog}
    onclick={(e) => e.stopPropagation()}
  >
    <h2>{title}</h2>
    {@render children?.()}
    {#if actions}<div class="actions">{@render actions()}</div>{/if}
  </div>
</div>

<style>
  .scrim {
    position: fixed;
    inset: 0;
    z-index: 30;
    display: flex;
    align-items: center;
    justify-content: center;
    background: var(--scrim);
  }
  .modal {
    width: 460px;
    max-width: 90vw;
    padding: 18px 20px;
    background: var(--panel);
    border: 1px solid var(--line-hi);
    border-radius: 8px;
  }
  h2 {
    margin: 0 0 8px;
    font-size: 16px;
  }
  .actions {
    display: flex;
    gap: 8px;
    justify-content: flex-end;
    margin-top: 16px;
  }
</style>
