<script lang="ts">
  // Always-mounted part of the device area's undo: the "Undo" prompt after an edit, and keeping
  // the edit history in step with the open library (picked up by shell/overlays.ts).
  import { session } from "$lib/state/session.svelte";
  import { undo } from "$lib/state/undo.svelte";
  import Button from "$lib/ui/Button.svelte";
  import { pillText } from "./trustLogic";

  // the history belongs to the library that is open: read it when one opens
  $effect(() => {
    const root = session.library?.root;
    if (root) void undo.refresh();
  });
</script>

{#if undo.pill}
  <div class="pill" role="status">
    <span class="text">{pillText(undo.pill.text)}</span>
    <Button small variant="primary" disabled={undo.busy} onclick={() => undo.undoNewest()}>Undo</Button>
    <Button small variant="ghost" aria-label="Dismiss" onclick={() => undo.dismissPill()}>×</Button>
  </div>
{/if}

<style>
  .pill {
    position: fixed;
    /* left, where toasts (right) are not, and just above the player bar whatever height it has */
    left: 14px;
    bottom: calc(var(--player-h, 48px) + 8px);
    z-index: 50;
    display: flex;
    align-items: center;
    gap: 8px;
    width: max-content;
    max-width: min(560px, calc(100vw - 28px));
    padding: 6px 8px 6px 12px;
    background: var(--panel);
    border: 1px solid var(--line-hi);
    border-radius: 6px;
  }
  .text {
    min-width: 0;
    overflow-wrap: anywhere;
  }
</style>
