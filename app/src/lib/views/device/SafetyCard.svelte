<script lang="ts">
  import type { TrustSettings } from "$lib/api/device";
  import { undo } from "$lib/state/undo.svelte";
  import Button from "$lib/ui/Button.svelte";
  import Card from "./Card.svelte";

  // The choices of the trust pack, kept in the app's config folder (not on the card). The defaults
  // are what the app uses until they are changed here.
  let draft = $state<TrustSettings>({ ...undo.settings });

  // the saved values replace the form when they are loaded or saved, never while typing
  $effect(() => {
    draft = { ...undo.settings };
  });

  const whole = (v: number, lo: number, hi: number) => Number.isInteger(v) && v >= lo && v <= hi;
  const valid = $derived(
    whole(draft.undo_limit, 0, undo.maxLimit) &&
      whole(draft.dead_row_guard_percent, 1, 100) &&
      whole(draft.undo_pill_seconds, 0, 120),
  );
  const changed = $derived(
    draft.undo_limit !== undo.settings.undo_limit ||
      draft.dead_row_guard_percent !== undo.settings.dead_row_guard_percent ||
      draft.undo_pill_seconds !== undo.settings.undo_pill_seconds,
  );
</script>

<Card title="Safety settings">
  <p class="muted">What the restore, undo and fix tools do by default. Saved on this computer, not on the Pacemaker.</p>
  {#if undo.settingsWarning}<p class="warn">{undo.settingsWarning}</p>{/if}
  <div class="grid">
    <label for="undo-limit">Edits kept for undo</label>
    <input id="undo-limit" type="number" min="0" max={undo.maxLimit} step="1" bind:value={draft.undo_limit} />
    <p class="hint">Per library, newest kept. 0 switches the history off.</p>

    <label for="undo-pill">Undo prompt stays for (seconds)</label>
    <input id="undo-pill" type="number" min="0" max="120" step="1" bind:value={draft.undo_pill_seconds} />
    <p class="hint">The note after an edit with an Undo button. 0 hides it; the Edit history card still lists everything.</p>

    <label for="dead-guard">Extra check above (% of tracks)</label>
    <input id="dead-guard" type="number" min="1" max="100" step="1" bind:value={draft.dead_row_guard_percent} />
    <p class="hint">
      Removing dead library rows asks you to confirm the right drive is fully mounted when more than this share of the
      tracks look dead.
    </p>
  </div>
  <div class="actions">
    <Button variant="primary" disabled={!valid || !changed} onclick={() => undo.saveSettings({ ...draft })}>Save</Button>
    <Button variant="ghost" disabled={!changed} onclick={() => (draft = { ...undo.settings })}>Revert</Button>
  </div>
</Card>

<style>
  .grid {
    display: grid;
    grid-template-columns: 1fr 90px;
    gap: 2px 10px;
    align-items: center;
  }
  input {
    background: var(--deep);
    color: var(--text);
    border: 1px solid var(--line);
    border-radius: 4px;
    padding: 3px 6px;
    font: inherit;
    width: 100%;
    box-sizing: border-box;
  }
  .hint {
    grid-column: 1 / -1;
    margin: 0 0 8px;
    font-size: 12px;
    color: var(--muted);
  }
  .warn {
    color: var(--bad-text);
    font-size: 12px;
  }
  .actions {
    display: flex;
    gap: 8px;
  }
</style>
