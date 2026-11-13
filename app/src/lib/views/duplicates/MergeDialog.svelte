<script lang="ts">
  // The review of a merge. Every kept track and every copy is named, with each change to its
  // case places, its row in the library and its files, and the list is the backend's own plan
  // (the job is started from it and refuses to run if the library changed). Removing the copies
  // from the library and deleting their files are two separate boxes, both off at first; the
  // second needs the first. Cancel is focused when the dialog opens (the effect below), so Enter never merges.
  import { mergeButtonLabel, planSentences, plural, totalsLine } from "$lib/state/dupeLogic";
  import { duplicates } from "$lib/state/duplicates.svelte";
  import Button from "$lib/ui/Button.svelte";

  const d = duplicates;
  const plan = $derived(d.plan);
  const groups = $derived(plan ? planSentences(plan) : []);
  const destructive = $derived(d.removeRows);
  // removing a row is the one write here no Pacemaker has been seen to accept yet: a setting turns it on
  const removalAllowed = $derived(d.settings.allow_row_removal);

  let dialog = $state<HTMLDivElement | null>(null);
  $effect(() => {
    // same rule as ui/Modal: the element marked data-autofocus (the safe choice) takes the focus on open
    if (d.mergeOpen && dialog) (dialog.querySelector<HTMLElement>("[data-autofocus]") ?? dialog).focus();
  });
</script>

<svelte:window onkeydown={(e) => d.mergeOpen && e.key === "Escape" && d.closeMerge()} />

{#if d.mergeOpen}
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
  <div class="scrim" onclick={() => d.closeMerge()}>
    <div class="modal" role="dialog" aria-modal="true" aria-label="Merge duplicates" tabindex="-1" bind:this={dialog} onclick={(e) => e.stopPropagation()}>
      <h2>Merge {plural(d.picked, "set")} of copies</h2>
      <p class="muted">
        Each set is folded into the track you chose to keep: the case places of the other copies move to it, and it takes the best rating and play count
        (as set in Settings). Nothing is written until you press the button at the bottom.
      </p>

      <div class="choices">
        <label class:dis={!removalAllowed}>
          <input type="checkbox" checked={d.removeRows} disabled={!removalAllowed} onchange={(e) => d.setRemoveRows(e.currentTarget.checked)} />
          <span>
            <b>Also remove the other copies from the library.</b> A copy that a mix uses stays, and is listed below.
            {#if removalAllowed}
              <span class="muted small">Deleting rows from the library has not been tried on a real Pacemaker yet; the app copies <code>music.db</code> first.</span>
            {:else}
              <span class="muted small">Switched off in Settings (“Allow removing copies from the library”): deleting rows has not been tried on a real Pacemaker yet.</span>
            {/if}
          </span>
        </label>
        <label class:dis={!d.removeRows}>
          <input type="checkbox" checked={d.deleteFiles} disabled={!d.removeRows} onchange={(e) => d.setDeleteFiles(e.currentTarget.checked)} />
          <span><b>Also delete their audio files from the Pacemaker.</b> This cannot be undone. A file another track still uses is kept.</span>
        </label>
      </div>

      {#if d.planError}<p class="bad" role="alert">{d.planError}</p>{/if}

      {#if plan}
        <p class="totals" class:stale={d.planning}>{totalsLine(plan)}</p>
        <div class="list" aria-label="Everything this merge changes" aria-busy={d.planning}>
          {#each groups as g, gi (gi)}
            <div class="grp">
              <div class="keep"><b>Keep</b> {g.keeper}</div>
              {#if g.stats}<div class="muted small">{g.stats}</div>{/if}
              <ul>
                {#each g.copies as c (c.track_id)}
                  <li>
                    <span class="cn">{c.label}</span>
                    <ul>
                      {#each c.parts as p, pi (pi)}
                        <li class:del={p.startsWith("deletes")} class:rem={p === "removed from the library"}>{p}</li>
                      {/each}
                    </ul>
                  </li>
                {/each}
              </ul>
            </div>
          {/each}
        </div>
        {#if plan.problems.length > 0}
          <ul class="warnings" role="alert" aria-label="Problems">
            {#each plan.problems as w, i (i)}<li>{w}</li>{/each}
          </ul>
        {/if}
        {#if plan.notes.length > 0}
          <ul class="notes" aria-label="Notes">
            {#each plan.notes as n, i (i)}<li>{n}</li>{/each}
          </ul>
        {/if}
      {:else if d.planning}
        <p class="muted">Working out what would change…</p>
      {/if}

      <div class="actions">
        <Button data-autofocus onclick={() => d.closeMerge()}>Cancel</Button>
        <span class="go" class:danger={destructive}>
          <Button variant="primary" disabled={!plan || d.planning || d.starting || plan.totals.groups === 0 || plan.problems.length > 0} onclick={() => d.confirmMerge()}>
            {mergeButtonLabel(plan, d.removeRows, d.deleteFiles)}
          </Button>
        </span>
      </div>
    </div>
  </div>
{/if}

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
    width: 780px;
    max-width: 94vw;
    max-height: 92vh;
    overflow: auto;
    padding: 18px 20px;
    background: var(--panel);
    border: 1px solid var(--line-hi);
    border-radius: 8px;
  }
  h2 {
    margin: 0 0 8px;
    font-size: 16px;
  }
  p {
    margin: 4px 0 8px;
  }
  .small {
    font-size: 12px;
  }
  .choices {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin: 10px 0;
    padding: 8px 10px;
    background: var(--deep);
    border: 1px solid var(--line);
    border-radius: 6px;
  }
  .choices label {
    display: flex;
    gap: 8px;
    align-items: flex-start;
  }
  .choices label.dis {
    opacity: 0.55;
  }
  .bad {
    color: var(--bad-text);
  }
  .totals {
    font-weight: 600;
  }
  .totals.stale {
    opacity: 0.5;
  }
  .list {
    max-height: 38vh;
    overflow: auto;
    padding: 6px 10px;
    background: var(--deep);
    border: 1px solid var(--line);
    border-radius: 4px;
  }
  .grp {
    padding: 6px 0;
    border-bottom: 1px solid var(--line);
  }
  .grp:last-child {
    border-bottom: 0;
  }
  ul {
    margin: 4px 0 0;
    padding-left: 20px;
  }
  .cn {
    font-weight: 600;
  }
  li.del,
  li.rem {
    color: var(--bad-text);
  }
  .warnings {
    margin: 8px 0 0;
    padding: 6px 10px 6px 28px;
    background: var(--bad-bg);
    border: 1px solid var(--bad);
    border-radius: 4px;
    color: var(--bad-text);
    font-weight: 600;
  }
  .notes {
    margin: 8px 0 0;
    color: var(--muted);
  }
  .actions {
    display: flex;
    gap: 8px;
    justify-content: flex-end;
    margin-top: 16px;
  }
  /* removing or deleting turns the button red, so it never looks like the harmless merge */
  .go.danger :global(button) {
    background: var(--bad-bg);
    border-color: var(--bad);
    color: var(--bad-text);
  }
</style>
