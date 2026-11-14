<script lang="ts">
  // The "Smart cases" section of the sidebar (a fixed slot rendered by shell/Sidebar.svelte).
  // Lists the saved smart cases with their live match counts; also loads the case list,
  // favourites and smart cases whenever a library is opened (this component is always mounted
  // while one is).
  import { untrack } from "svelte";
  import { bg } from "$lib/state/bganalysis.svelte";
  import { cases } from "$lib/state/cases.svelte";
  import { nav } from "$lib/state/nav.svelte";
  import { session } from "$lib/state/session.svelte";
  import CaseDrops from "./CaseDrops.svelte";

  $effect(() => {
    if (!session.library?.root) return;
    untrack(() => {
      cases.refresh();
      cases.loadSmart();
    });
  });

  // A rating, a key or a play count changes the library's tracks without reopening it, and a smart
  // case can rule on any of them: count again (after a pause, so a run of edits is one count).
  // Not on the first run for a library: opening it already counts. While background analysis is
  // still working it writes a key per track, so the count waits and runs once when the queue drains.
  let seen: { root: string; tracks: unknown } | null = null;
  let waiting = false;
  const busy = $derived(bg.status.left > 0);
  $effect(() => {
    const root = session.library?.root;
    const tracks = session.library?.tracks;
    untrack(() => {
      if (!root || !tracks) {
        seen = null;
        waiting = false;
      } else {
        if (seen && seen.root === root && seen.tracks !== tracks) {
          if (busy) waiting = true;
          else cases.scheduleSmartCounts();
        }
        seen = { root, tracks };
      }
    });
  });
  $effect(() => {
    if (busy) return;
    untrack(() => {
      if (!waiting) return;
      waiting = false;
      cases.scheduleSmartCounts();
    });
  });

  function open(id: number) {
    if (nav.view === "cases" && cases.smartOpen === id) return;
    // asks first if the editor now open holds unsaved edits
    cases.leave(() => {
      cases.openSmart(id);
      nav.go("cases");
    });
  }
</script>

<!-- the drop strip that shows while tracks are being dragged (see CaseDrops.svelte) -->
<CaseDrops />

<div class="sect head">
  <span>Smart cases</span>
  <button class="new" onclick={() => open(0)} title="Build a smart case from rules">+ New</button>
</div>
{#each cases.smart as s (s.id)}
  <button class="item" class:active={nav.view === "cases" && cases.smartOpen === s.id} onclick={() => open(s.id)}>
    <span class="label">
      <span class="ic" aria-hidden="true">{s.on_device ? "▣" : "◇"}</span>{s.name}
    </span>
    {#if cases.smartErrors[s.id]}
      <span class="n bad" title={cases.smartErrors[s.id]} aria-label={`Cannot run: ${cases.smartErrors[s.id]}`}>!</span>
    {:else}
      <span class="n">{cases.smartCounts[s.id] ?? ""}</span>
    {/if}
  </button>
{:else}
  <p class="note">None yet. Rules like “rated 4+ and never played”.</p>
{/each}

<style>
  .head {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
  }
  .new {
    background: none;
    border: none;
    padding: 0;
    font: inherit;
    font-size: 11px;
    text-transform: none;
    letter-spacing: 0;
    color: var(--accent);
    cursor: pointer;
  }
  .item {
    display: flex;
    justify-content: space-between;
    gap: 6px;
    border: none;
    background: none;
    border-radius: 4px;
    padding: 5px 10px;
    color: var(--text);
    font: inherit;
    text-align: left;
    cursor: pointer;
  }
  .item:hover {
    background: var(--hover);
  }
  .item.active {
    background: var(--accent-soft);
    color: var(--accent);
  }
  .label {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .ic {
    margin-right: 6px;
    color: var(--muted);
  }
  .n {
    color: var(--muted);
  }
  .n.bad {
    color: var(--bad);
    font-weight: 700;
  }
</style>
