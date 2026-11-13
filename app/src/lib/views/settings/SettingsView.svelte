<script lang="ts">
  import { toast } from "$lib/state/toast.svelte";
  import Button from "$lib/ui/Button.svelte";
  import { settingsEntries } from "./registry";
  import { group } from "./settingsLogic";
  import type { SettingsEntry } from "./types";

  // Every feature setting in one place, grouped by area. Nothing here is a second copy of a control:
  // each entry shows the feature's own settings component (or opens its own dialog or panel), so a
  // value changed here is the value changed where the feature is used, and the other way round.
  let query = $state("");
  let opened = $state<Record<string, boolean>>({});
  let dialogId = $state<string | null>(null);
  let loading = $state<string | null>(null);

  const groups = $derived(group(settingsEntries, query));
  const total = $derived(groups.reduce((n, g) => n + g.entries.length, 0));
  const dialog = $derived(settingsEntries.find((e) => e.id === dialogId && e.how === "dialog") ?? null);

  /** Runs the entry's loader (its saved values); false if it failed, with the reason shown. */
  async function ready(e: SettingsEntry): Promise<boolean> {
    loading = e.id;
    try {
      await e.load?.();
      return true;
    } catch (err) {
      toast.show(`Could not read the settings of ${e.title}: ${err}`, "bad", 8000);
      return false;
    } finally {
      loading = null;
    }
  }

  async function toggle(e: SettingsEntry) {
    if (opened[e.id]) {
      opened[e.id] = false;
    } else if (await ready(e)) {
      opened[e.id] = true;
    }
  }

  async function act(e: SettingsEntry) {
    if (e.how === "open") {
      if (await ready(e)) await e.open();
    } else if (e.how === "dialog") {
      if (await ready(e)) dialogId = e.id;
    }
  }
</script>

<div class="bar">
  <h2>Settings</h2>
  <input type="search" class="find" placeholder="Search the settings" aria-label="Search the settings" bind:value={query} />
  <span class="muted">{total} {total === 1 ? "section" : "sections"}</span>
</div>

<div class="page">
  <p class="lead muted">
    These are the same controls you find where each feature is used; changing one here changes it there. They are
    kept on this computer, not on the Pacemaker.
  </p>

  {#each groups as g (g.area)}
    <section class="area" aria-labelledby="area-{g.area.replace(/\s+/g, '-')}">
      <h3 id="area-{g.area.replace(/\s+/g, '-')}">{g.area}</h3>
      {#each g.entries as e (e.id)}
        <article class="entry" id="settings-{e.id}">
          <header>
            <div class="what">
              {#if e.how === "inline"}
                <button class="name" aria-expanded={!!opened[e.id]} aria-controls="settings-body-{e.id}" onclick={() => toggle(e)}>
                  <span class="caret" aria-hidden="true">{opened[e.id] ? "▾" : "▸"}</span>{e.title}
                </button>
              {:else}
                <span class="name plain">{e.title}</span>
              {/if}
              <span class="help muted">{e.help}</span>
              {#if e.how === "dialog" || e.how === "open"}
                {@const line = e.summary?.()}
                {#if line}<span class="sum">{line}</span>{/if}
              {/if}
            </div>
            {#if e.how !== "inline"}
              <Button small disabled={loading === e.id} onclick={() => act(e)}>
                {e.how === "dialog" ? "Change…" : e.button}
              </Button>
            {/if}
          </header>
          {#if e.how === "inline" && opened[e.id]}
            {@const C = e.component}
            <div class="body" id="settings-body-{e.id}"><C {...e.props ?? {}} /></div>
          {/if}
        </article>
      {/each}
    </section>
  {:else}
    <p class="empty">No setting matches “{query}”.</p>
  {/each}
</div>

{#if dialog && dialog.how === "dialog"}
  {@const D = dialog.component}
  <D onclose={() => (dialogId = null)} />
{/if}

<style>
  .bar {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 7px 12px;
    border-bottom: 1px solid var(--line);
  }
  h2 {
    margin: 0 8px 0 0;
    font-size: 15px;
  }
  .find {
    width: 260px;
    max-width: 100%;
  }
  .page {
    padding: 12px;
    max-width: 920px;
  }
  .lead {
    margin: 0 0 14px;
  }
  .area {
    margin-bottom: 18px;
  }
  h3 {
    margin: 0 0 6px;
    font-size: 11px;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--muted);
  }
  .entry {
    border: 1px solid var(--line);
    border-radius: 6px;
    background: var(--panel);
    margin-bottom: 6px;
  }
  header {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 12px;
    padding: 8px 10px;
  }
  .what {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }
  .name {
    align-self: flex-start;
    border: none;
    background: none;
    padding: 0;
    color: var(--text);
    font: inherit;
    font-weight: 600;
    cursor: pointer;
    text-align: left;
  }
  .name.plain {
    cursor: default;
  }
  .caret {
    display: inline-block;
    width: 1.1em;
    color: var(--muted);
  }
  .help {
    font-size: 12px;
  }
  .sum {
    font-size: 12px;
    color: var(--green);
  }
  .body {
    padding: 4px 10px 12px 10px;
    border-top: 1px solid var(--line);
  }
  .empty {
    color: var(--muted);
  }
</style>
