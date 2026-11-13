<script lang="ts">
  import { deviceUndoClear } from "$lib/api/device";
  import { session } from "$lib/state/session.svelte";
  import { toast } from "$lib/state/toast.svelte";
  import { undo } from "$lib/state/undo.svelte";
  import Button from "$lib/ui/Button.svelte";
  import Card from "./Card.svelte";
  import { device } from "./device.svelte";
  import { plural } from "./format";
  import { ageText } from "./trustLogic";
  import UndoModal from "./UndoModal.svelte";

  // The edits made to ratings, tags and favourites since this library was opened, newest first.
  // "Undo back to here" reverses that edit and every newer one, after a preview that names each
  // change and anything it will leave alone because it was changed again afterwards.
  let count = $state(0);
  let now = $state(Date.now());

  $effect(() => {
    void undo.entries;
    now = Date.now();
  });

  const kindText = { rating: "Rating", metadata: "Tags", favourite: "Favourite" } as const;

  async function clearAll() {
    const root = session.library?.root;
    if (!root) return;
    try {
      await deviceUndoClear(root);
    } catch (e) {
      toast.show(String(e), "bad", 8000);
    }
  }
</script>

<Card title="Edit history">
  {#snippet aside()}
    {#if undo.entries.length > 0}<Button small variant="ghost" onclick={clearAll}>Clear</Button>{/if}
  {/snippet}
  <p class="muted">
    Ratings, tag edits and favourites made in this session can be put back. Only a field that still holds the value the
    edit gave it is reverted; anything changed again since is left alone and named first. The history is kept in memory
    for this library and is dropped when {session.isComputer ? 'the library is closed' : 'the card is ejected'} or restored.
  </p>
  {#if undo.settings.undo_limit === 0}
    <p class="muted">The edit history is switched off (see Safety settings).</p>
  {:else if undo.entries.length === 0}
    <p class="muted">Nothing to undo yet. Rate a track, edit its tags or tap a heart and it appears here.</p>
  {:else}
    <ul class="rows" aria-label="Edit history">
      {#each undo.entries as e, i (e.id)}
        <li>
          <span class="kind">{kindText[e.kind]}</span>
          <div class="what">
            <span>{e.summary}</span>
            <span class="muted">{ageText(e.at_ms, now)}{e.tracks > 1 ? ` · ${plural(e.tracks, "track")}` : ""}</span>
          </div>
          <Button small disabled={device.ejecting || undo.busy} onclick={() => (count = i + 1)}>
            {i === 0 ? "Undo…" : "Undo back to here…"}
          </Button>
        </li>
      {/each}
    </ul>
    <p class="muted fine">Keeps the last {undo.settings.undo_limit} edits.</p>
  {/if}
</Card>

{#if count > 0}
  <UndoModal {count} onclose={() => (count = 0)} />
{/if}

<style>
  .rows {
    list-style: none;
    margin: 8px 0 0;
    padding: 0;
    border: 1px solid var(--line);
    border-radius: 4px;
    max-height: 300px;
    overflow: auto;
  }
  li {
    display: grid;
    grid-template-columns: 62px 1fr auto;
    gap: 8px;
    align-items: center;
    padding: 5px 8px;
  }
  li:nth-child(odd) {
    background: var(--row);
  }
  .kind {
    font-size: 11px;
    color: var(--accent);
  }
  .what {
    display: flex;
    flex-direction: column;
    min-width: 0;
    overflow-wrap: anywhere;
  }
  .what .muted {
    font-size: 11px;
  }
  .fine {
    font-size: 12px;
    margin: 6px 0 0;
  }
</style>
