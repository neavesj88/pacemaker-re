<script lang="ts">
  // The right-hand panel: shows the selected clip, tool, loop / region or crossfade, or the mix
  // itself when nothing is selected. The panels share the field styles defined here.
  import * as E from "$lib/state/mixedit";
  import { mixes } from "$lib/state/mixes.svelte";
  import ClipPanel from "./ClipPanel.svelte";
  import MixPanel from "./MixPanel.svelte";
  import ProcessPanel from "./ProcessPanel.svelte";
  import TransitionPanel from "./TransitionPanel.svelte";
  import { describe } from "./actions";

  let { ondelete }: { ondelete: () => void } = $props();

  const found = $derived.by(() => {
    const doc = mixes.doc;
    if (!doc || mixes.selected.length !== 1) return null;
    return E.find(doc, mixes.selected[0]);
  });
</script>

<aside class="insp" aria-label="Inspector">
  {#if mixes.selected.length > 1 && mixes.doc}
    <h3>{mixes.selected.length} items selected</h3>
    <ul class="names">
      {#each mixes.selected.slice(0, 12) as id (id)}<li>{describe(mixes.doc, id)}</li>{/each}
      {#if mixes.selected.length > 12}<li class="muted">and {mixes.selected.length - 12} more</li>{/if}
    </ul>
    <div class="buttons">
      <button onclick={() => mixes.copy()}>Copy</button>
      <button class="danger" onclick={ondelete}>Delete…</button>
    </div>
    <p class="note">Drag any selected clip to move them together. Shift- or Ctrl-click adds to the selection.</p>
  {:else if found?.kind === "clip"}
    {#key found.clip.id}<ClipPanel clip={found.clip} {ondelete} />{/key}
  {:else if found?.kind === "process"}
    {#key found.process.id}<ProcessPanel clip={found.clip} process={found.process} {ondelete} />{/key}
  {:else if found?.kind === "transition"}
    {#key found.transition.id}<TransitionPanel t={found.transition} {ondelete} />{/key}
  {:else}
    <MixPanel />
  {/if}
</aside>

<style>
  .insp {
    width: 292px;
    flex: 0 0 292px;
    min-height: 0;
    overflow: auto;
    border-left: 1px solid var(--line);
    background: var(--panel);
    padding: 12px;
    box-sizing: border-box;
  }
  .insp :global(h3) {
    margin: 0 0 2px;
    font-size: 14px;
    overflow-wrap: anywhere;
  }
  .insp :global(h4) {
    margin: 14px 0 4px;
    font-size: 12px;
    color: var(--muted);
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }
  .insp :global(.field) {
    display: grid;
    grid-template-columns: 88px 1fr;
    gap: 6px;
    align-items: center;
    margin: 6px 0;
  }
  .insp :global(.field > label) {
    color: var(--muted);
    font-size: 12px;
  }
  .insp :global(.field input[type="number"]),
  .insp :global(.field input[type="text"]),
  .insp :global(.field select) {
    width: 100%;
    box-sizing: border-box;
    min-width: 0;
  }
  .insp :global(.field input[type="range"]) {
    width: 100%;
    min-width: 0;
  }
  .insp :global(.buttons) {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-top: 10px;
  }
  .insp :global(.danger) {
    color: var(--bad);
  }
  .insp :global(.note) {
    font-size: 12px;
    color: var(--muted);
    border-top: 1px solid var(--line);
    margin: 14px 0 0;
    padding-top: 10px;
  }
  .insp :global(.warn) {
    background: var(--bad-bg);
    color: var(--bad-text);
    border-radius: 4px;
    padding: 6px 8px;
    font-size: 12px;
    margin: 8px 0;
  }
  .insp :global(.seg) {
    display: inline-flex;
    gap: 0;
  }
  .insp :global(.seg button) {
    border-radius: 0;
    padding: 3px 10px;
  }
  .insp :global(.seg button:first-child) {
    border-radius: 4px 0 0 4px;
  }
  .insp :global(.seg button:last-child) {
    border-radius: 0 4px 4px 0;
  }
  .insp :global(.seg button.on) {
    background: var(--accent-soft);
    color: var(--accent);
    border-color: var(--accent);
  }
  .insp :global(.val) {
    font-variant-numeric: tabular-nums;
    color: var(--text);
  }
  .insp :global(.curvebox) {
    background: var(--deep);
    border: 1px solid var(--line);
    border-radius: 4px;
    overflow: hidden;
  }
  .names {
    margin: 8px 0;
    padding-left: 18px;
    font-size: 12px;
    color: var(--text);
  }
  .muted {
    color: var(--muted);
  }
</style>
