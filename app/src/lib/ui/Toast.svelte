<script lang="ts">
  import { toast } from "$lib/state/toast.svelte";
</script>

<div class="toasts" aria-live="polite">
  {#each toast.items as t (t.id)}
    <div class="toast {t.kind}">{t.text}</div>
  {/each}
</div>

<style>
  .toasts {
    position: fixed;
    right: 14px;
    /* just above the player bar, whatever height it has (Player.svelte publishes --player-h) */
    bottom: calc(var(--player-h, 48px) + 8px);
    z-index: 50;
    display: flex;
    flex-direction: column;
    gap: 6px;
    pointer-events: none;
  }
  .toast {
    padding: 6px 12px;
    background: var(--panel);
    border: 1px solid var(--line-hi);
    border-radius: 6px;
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
