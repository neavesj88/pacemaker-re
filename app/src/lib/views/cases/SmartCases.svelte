<script lang="ts">
  // The "Smart cases" section of the sidebar (a fixed slot rendered by shell/Sidebar.svelte).
  // Lists the saved smart cases with their live match counts; also loads the case list,
  // favourites and smart cases whenever a library is opened (this component is always mounted
  // while one is).
  import { untrack } from "svelte";
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
