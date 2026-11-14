<script lang="ts">
  import { session } from "$lib/state/session.svelte";
  import { toast } from "$lib/state/toast.svelte";

  // The stack sits above the player bar, whatever height the owner dragged it to: it measures the
  // bar (when there is one) and keeps following it, so a message never covers the player controls.
  const GAP = 8;
  let lift = $state(0);

  $effect(() => {
    void toast.items.length;
    // the player bar mounts when a library opens, which may be after the message appeared
    void session.library;
    const bar = document.querySelector<HTMLElement>("footer.player");
    if (!bar) {
      lift = 0;
      return;
    }
    const measure = () => (lift = Math.ceil(bar.getBoundingClientRect().height));
    measure();
    const ro = new ResizeObserver(measure);
    ro.observe(bar);
    return () => ro.disconnect();
  });
</script>

<div class="toasts" aria-live="polite" style="bottom: {lift + GAP}px">
  {#each toast.items as t (t.id)}
    <div class="toast {t.kind}" role={t.kind === "bad" ? "alert" : "status"}>
      <span class="text">{t.text}</span>
      <button class="close" aria-label="Dismiss this message" title="Dismiss" onclick={() => toast.dismiss(t.id)}>×</button>
    </div>
  {/each}
</div>

<style>
  .toasts {
    position: fixed;
    right: 14px;
    z-index: 50;
    display: flex;
    flex-direction: column;
    gap: 6px;
    max-width: min(560px, calc(100vw - 28px));
    pointer-events: none;
  }
  .toast {
    display: flex;
    align-items: flex-start;
    gap: 10px;
    padding: 6px 8px 6px 12px;
    background: var(--panel);
    border: 1px solid var(--line-hi);
    border-radius: 6px;
    pointer-events: auto;
  }
  .text {
    flex: 1;
    overflow-wrap: anywhere;
  }
  .close {
    flex: none;
    background: none;
    border: 0;
    color: inherit;
    font: inherit;
    line-height: 1;
    padding: 0 4px;
    cursor: pointer;
    opacity: 0.7;
  }
  .close:hover,
  .close:focus-visible {
    opacity: 1;
  }
  .ok {
    border-color: var(--ok);
  }
  .bad {
    background: var(--bad-bg);
    border-color: var(--bad);
    color: var(--bad-text);
  }
</style>
