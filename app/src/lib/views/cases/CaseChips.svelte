<script lang="ts">
  // The cases that hold one track, as buttons that open the case. Used in the Tracks inspector.
  // It reads the library when the track changes and again when the case list changes (a track was
  // added to or removed from a case, a case was renamed or deleted).
  import { casesHolding, type TrackHolding } from "$lib/api/cases";
  import { cases } from "$lib/state/cases.svelte";
  import { holders } from "$lib/state/holders.svelte";
  import { session } from "$lib/state/session.svelte";
  import { holdingRows } from "./holdersLogic";

  let { trackId }: { trackId: number } = $props();

  const SHOWN = 4;
  let holding = $state.raw<TrackHolding | null>(null);
  let failed = $state(false);

  $effect(() => {
    const root = session.library?.root;
    const id = trackId;
    void cases.infos; // reload after case edits
    void cases.smart;
    if (!root) return;
    let current = true;
    casesHolding(root, [id])
      .then((r) => {
        if (current) {
          holding = r[0] ?? null;
          failed = false;
        }
      })
      .catch(() => {
        if (current) failed = true;
      });
    return () => {
      current = false;
    };
  });

  const rows = $derived(holding && holding.track_id === trackId ? holdingRows(holding) : null);
  const all = $derived(
    rows
      ? [
          ...rows.cases.map((c) => ({ key: `c${c.case_id}`, name: c.name, title: c.note, open: () => holders.goToCase(c.case_id) })),
          ...rows.smart.map((s) => ({ key: `s${s.id}`, name: s.name, title: s.note, open: () => holders.goToSmart(s.id) })),
        ]
      : [],
  );
</script>

{#if failed}
  <span class="muted">unavailable</span>
{:else if !rows}
  <span class="muted">…</span>
{:else if all.length === 0}
  <span class="muted">none</span>
{:else}
  <span class="chips">
    {#each all.slice(0, SHOWN) as c (c.key)}
      <button type="button" class="chip" title={c.title} onclick={c.open}>{c.name}</button>
    {/each}
    {#if all.length > SHOWN}
      <button type="button" class="chip more" onclick={() => holders.show(trackId)}>+{all.length - SHOWN} more…</button>
    {/if}
  </span>
{/if}

<style>
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
  }
  .chip {
    max-width: 150px;
    padding: 1px 7px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    background: var(--deep);
    border: 1px solid var(--line-hi);
    border-radius: 10px;
    color: var(--text);
    font: inherit;
    font-size: 11px;
    cursor: pointer;
  }
  .chip:hover,
  .chip:focus-visible {
    border-color: var(--accent);
  }
  .more {
    color: var(--muted);
  }
</style>
