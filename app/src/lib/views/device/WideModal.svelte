<script lang="ts">
  import type { Snippet } from "svelte";

  // ui/Modal.svelte with a width: the trust-pack dialogs list file paths and track names that do
  // not fit in 460px. Same behaviour (Escape or the scrim closes it; data-autofocus takes focus)
  // and the same look. ui/ is integration-owned, so this lives here instead of widening Modal.
  let {
    title,
    onclose,
    width = 680,
    children,
    actions,
  }: { title: string; onclose: () => void; width?: number; children?: Snippet; actions?: Snippet } = $props();

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
    style:width="{width}px"
    onclick={(e) => e.stopPropagation()}
  >
    <h2>{title}</h2>
    <div class="body">{@render children?.()}</div>
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
    max-width: 94vw;
    max-height: 90vh;
    display: flex;
    flex-direction: column;
    padding: 18px 20px;
    background: var(--panel);
    border: 1px solid var(--line-hi);
    border-radius: 8px;
  }
  h2 {
    margin: 0 0 8px;
    font-size: 16px;
  }
  .body {
    min-height: 0;
    overflow: auto;
  }
  .actions {
    display: flex;
    gap: 8px;
    justify-content: flex-end;
    margin-top: 16px;
  }
</style>
