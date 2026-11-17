<script lang="ts">
  import type { TrustSettings } from "$lib/api/device";
  import { undo } from "$lib/state/undo.svelte";
  import Button from "$lib/ui/Button.svelte";
  import HelpLink from "../help/HelpLink.svelte";
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
  {#snippet aside()}<HelpLink id="device-page" anchor="safety-settings" />{/snippet}
  <p class="muted">Saved on this computer.</p>
  {#if undo.settingsWarning}<p class="warn">{undo.settingsWarning}</p>{/if}
  <div class="grid">
    <label for="undo-limit">Edits kept for undo</label>
    <input id="undo-limit" type="number" min="0" max={undo.maxLimit} step="1" bind:value={draft.undo_limit} />
    <p class="hint">Per library. 0 turns history off.</p>

    <label for="undo-pill">Undo prompt stays for (seconds)</label>
    <input id="undo-pill" type="number" min="0" max="120" step="1" bind:value={draft.undo_pill_seconds} />
    <p class="hint">0 hides the prompt.</p>

    <label for="dead-guard">Extra check above (% of tracks)</label>
    <input id="dead-guard" type="number" min="1" max="100" step="1" bind:value={draft.dead_row_guard_percent} />
    <p class="hint">
      Asks to confirm the drive is mounted when more tracks look dead.
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
