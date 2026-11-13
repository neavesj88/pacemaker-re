<script lang="ts">
  import { untrack } from "svelte";
  import type { UndoPreview } from "$lib/api/device";
  import { undo } from "$lib/state/undo.svelte";
  import Button from "$lib/ui/Button.svelte";
  import { plural } from "./format";
  import { planLine } from "./trustLogic";
  import WideModal from "./WideModal.svelte";

  // The named confirmation of an undo of the newest `count` edits: every edit by its summary, and
  // every field it would put back (or leave alone, with the reason). The apply carries the id of
  // the newest edit this preview saw, and refuses if the history moved on.
  let { count, onclose }: { count: number; onclose: () => void } = $props();

  let pv = $state<UndoPreview | null>(null);
  let error = $state("");

  // an answer that arrives after the dialog was closed or `count` changed is dropped
  $effect(() => {
    let current = true;
    undo.preview(count).then(
      (p) => current && (pv = p),
      (e) => current && (error = String(e)),
    );
    return () => (current = false);
  });

  // the edits as they stood when the dialog opened; a newer edit made meanwhile makes the apply refuse
  const covered = untrack(() => undo.entries.slice(0, count));
  // lines of one track together, in the order the plan gives them
  const byTrack = $derived.by(() => {
    const groups: { track_id: number; label: string; lines: UndoPreview["plan"]["items"] }[] = [];
    for (const it of pv?.plan.items ?? []) {
      let g = groups.find((x) => x.track_id === it.track_id);
      if (!g) groups.push((g = { track_id: it.track_id, label: it.label, lines: [] }));
      g.lines.push(it);
    }
    return groups;
  });

  async function run() {
    if (!pv) return;
    const r = await undo.apply(pv.count, pv.top_entry_id);
    if (r) onclose();
  }
</script>

<WideModal title={`Undo ${plural(count, "edit")}?`} {onclose} width={640}>
  <p class="muted">The edits below are reversed, newest first. Nothing else on the card changes.</p>
  <ol class="edits" aria-label="Edits to undo">
    {#each covered as e (e.id)}<li>{e.summary}</li>{/each}
  </ol>

  {#if error}
    <p class="err" role="alert">{error}</p>
  {:else if !pv}
    <p class="muted">Checking what each field holds now…</p>
  {:else}
    <p>
      <b>{plural(pv.plan.reverts, "change")} will be put back</b>{#if pv.plan.skipped > 0}, {pv.plan.skipped} left alone{/if}
    </p>
    <div class="groups">
      {#each byTrack as g (g.track_id)}
        <div class="g">
          <div class="who">{g.label}</div>
          <ul>
            {#each g.lines as it, i (i)}
              <li class:skip={!!it.skip}>{planLine(it)}</li>
            {/each}
          </ul>
        </div>
      {/each}
    </div>
  {/if}

  {#snippet actions()}
    <Button data-autofocus onclick={onclose}>Cancel</Button>
    <Button variant="primary" disabled={!pv || pv.plan.reverts === 0 || undo.busy || !!error} onclick={run}>
      {undo.busy ? "Undoing…" : pv ? `Undo ${plural(pv.plan.reverts, "change")}` : "Undo"}
    </Button>
  {/snippet}
</WideModal>

<style>
  .edits {
    margin: 6px 0 10px;
    padding-left: 22px;
  }
  .groups {
    max-height: 260px;
    overflow: auto;
    border: 1px solid var(--line);
    border-radius: 4px;
  }
  .g {
    padding: 4px 8px;
  }
  .g:nth-child(odd) {
    background: var(--row);
  }
  .who {
    font-weight: 600;
    overflow-wrap: anywhere;
  }
  ul {
    margin: 2px 0 0;
    padding-left: 18px;
  }
  li.skip {
    color: var(--accent);
  }
  .err {
    color: var(--bad-text);
    background: var(--bad-bg);
    border: 1px solid var(--bad);
    border-radius: 4px;
    padding: 6px 8px;
    overflow-wrap: anywhere;
  }
</style>
