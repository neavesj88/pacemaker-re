<script lang="ts">
  import { cases } from "$lib/state/cases.svelte";
  import { nav } from "$lib/state/nav.svelte";
  import { session } from "$lib/state/session.svelte";
  import SmartCases from "$lib/views/cases/SmartCases.svelte";
  import NeedsAnalysis from "$lib/views/tracks/NeedsAnalysis.svelte";
  import { views } from "$lib/views/registry";
  import type { ViewId } from "$lib/views/ids";

  /** Pressing the entry of the view already showing goes back to its list: for Cases that leaves an
   *  open case or smart case editor (asking first about unsaved edits). */
  function pick(id: ViewId) {
    if (id === nav.view && id === "cases" && (cases.open || cases.smartOpen !== null)) cases.leave(() => cases.close());
    else nav.go(id);
  }
</script>

{#snippet section(id: "library" | "tools", title: string)}
  <div class="sect">{title}</div>
  {#each views.filter((v) => v.section === id && (!v.visible || v.visible())) as v (v.id)}
    <button class:active={nav.view === v.id} onclick={() => pick(v.id)}>
      {v.title ? v.title() : v.label}
      {#if v.count && session.library}<span>{v.count(session.library)}</span>{/if}
    </button>
  {/each}
{/snippet}

<nav>
  {@render section("library", "Library")}
  <SmartCases />
  {@render section("tools", "Tools")}

  <div class="spacer"></div>
  <NeedsAnalysis />
</nav>

<style>
  /* published so the undo prompt can sit past it */
  :global(:root) {
    --nav-w: 207px; /* outer width: the old 190px content + 16px padding + 1px border, so the layout is unchanged */
  }
  nav {
    box-sizing: border-box; /* --nav-w is the outer width, padding and border included */
    width: var(--nav-w);
    flex: none;
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 10px 8px;
    background: var(--panel);
    border-right: 1px solid var(--line);
    overflow: auto;
  }
  nav :global(.sect) {
    margin: 10px 10px 2px;
    font-size: 11px;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--muted);
  }
  nav :global(.sect:first-child) {
    margin-top: 0;
  }
  nav :global(.note) {
    margin: 0 10px;
    font-size: 12px;
    color: var(--muted);
  }
  button {
    display: flex;
    justify-content: space-between;
    border: none;
    background: none;
    border-radius: 4px;
    padding: 5px 10px;
    color: var(--text);
    font: inherit;
    text-align: left;
    cursor: pointer;
  }
  button:hover {
    background: var(--hover);
  }
  button span {
    color: var(--muted);
  }
  button.active {
    background: var(--accent-soft);
    color: var(--accent);
  }
  .spacer {
    flex: 1;
  }
  nav :global(.stat) {
    color: var(--muted);
    font-size: 12px;
    padding: 0 10px;
  }
  nav :global(.stat.warn) {
    color: var(--accent);
  }
  nav :global(.btn.small) {
    margin: 2px 6px;
  }
  /* a short window (a 1080p laptop with the taskbar): tighter rows, so the Tools section down to Settings fits without scrolling */
  @media (max-height: 800px) {
    nav {
      gap: 0;
      padding: 6px 8px;
    }
    button {
      padding: 3px 10px;
    }
    nav :global(.sect) {
      margin-top: 6px;
    }
  }
</style>
