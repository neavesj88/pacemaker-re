<script lang="ts">
  // The mixes on the device: open, create, rename, delete, import and export .pmk.
  import type { MixSummary } from "$lib/api/mixes";
  import { mixes } from "$lib/state/mixes.svelte";
  import { session } from "$lib/state/session.svelte";
  import { toast } from "$lib/state/toast.svelte";
  import Button from "$lib/ui/Button.svelte";
  import Stars from "$lib/ui/Stars.svelte";
  import Table from "$lib/ui/Table.svelte";
  import { errText } from "./actions";
  import { deleteMix, exportPmk, importPmk, newMix, openMix, renameMix } from "./flow";
  import { mmss } from "./layout";
  import { listMeta } from "./listmeta.svelte";
  import { mv } from "./view.svelte";
  import { setlist } from "../setlist/setlist.svelte";

  let picked = $state<number | null>(null);
  let sortKey = $state("name");
  let sortAsc = $state(true);
  let loading = $state(true);

  // refresh when the library changes (an in-place reload keeps the view)
  $effect(() => {
    void session.library;
    loading = true;
    mixes
      .refreshList()
      .catch((e) => toast.show(`Could not list the mixes: ${errText(e)}`, "bad", 8000))
      .finally(() => (loading = false));
  });

  // date and rating come from the library row, read in the background
  $effect(() => {
    void listMeta.fill(mixes.list.map((m) => m.id));
  });

  const columns = [
    { key: "name", label: "Name" },
    { key: "play_time_secs", label: "Length", num: true },
    { key: "track_count", label: "Tracks", num: true },
    { key: "date", label: "Modified", num: true },
    { key: "rating", label: "Rating" },
  ];

  const dateOf = (id: number) => listMeta.byId.get(id)?.dateModified ?? 0;
  const ratingOf = (id: number) => listMeta.byId.get(id)?.rating ?? 0;

  const rows = $derived.by(() => {
    const r = [...mixes.list];
    const dir = sortAsc ? 1 : -1;
    r.sort((a, b) => {
      switch (sortKey) {
        case "date":
          return (dateOf(a.id) - dateOf(b.id)) * dir;
        case "rating":
          return (ratingOf(a.id) - ratingOf(b.id)) * dir;
        case "name":
          return a.name.localeCompare(b.name, undefined, { numeric: true, sensitivity: "base" }) * dir;
        default:
          return ((a as Record<string, number | string>)[sortKey] as number) - ((b as Record<string, number | string>)[sortKey] as number) || 0;
      }
    });
    return r;
  });

  const current = $derived(mixes.list.find((m) => m.id === picked) ?? null);

  function sort(key: string) {
    if (sortKey === key) sortAsc = !sortAsc;
    else {
      sortKey = key;
      sortAsc = key === "name";
    }
  }

  const fmtDate = (ms: number) => (ms ? new Date(ms).toLocaleDateString(undefined, { year: "numeric", month: "short", day: "numeric" }) : "–");

  async function rate(m: MixSummary, r: number) {
    try {
      await listMeta.rate(m.id, r);
    } catch (e) {
      toast.show(`Could not set the rating: ${errText(e)}`, "bad", 7000);
    }
  }
</script>

<div class="list">
  <div class="bar">
    <h2>Mixes</h2>
    <span class="chip">{mixes.list.length} on this device</span>
    <Button variant="primary" onclick={newMix}>New mix</Button>
    <Button onclick={importPmk}>Import .pmk…</Button>
    <span class="sep"></span>
    <Button disabled={!current} onclick={() => current && openMix(current.id)}>Open</Button>
    <Button disabled={!current} onclick={() => current && renameMix(current)}>Rename…</Button>
    <Button disabled={!current} onclick={() => current && void exportPmk(current)}>Export .pmk…</Button>
    <Button disabled={!current} onclick={() => current && setlist.openMix(current.id)} title="A printable setlist of the picked mix">Setlist…</Button>
    <Button disabled={!current} onclick={() => current && deleteMix(current)}><span class:bad={!!current}>Delete…</span></Button>
  </div>

  {#if mv.pending.length > 0}
    <div class="banner">
      Choose a mix to add {mv.pending.length} track{mv.pending.length === 1 ? "" : "s"} to, or create a new one.
      <Button variant="link" onclick={() => (mv.pending = [])}>Cancel</Button>
    </div>
  {/if}

  <div class="body">
    {#if mixes.list.length > 0}
      <Table
        {columns}
        {rows}
        rowKey={(m) => m.id}
        {sortKey}
        {sortAsc}
        onsort={sort}
        rowClass={(m) => (m.id === picked ? "picked" : "")}
        onrowclick={(m) => (picked = m.id)}
      >
        {#snippet cell(m, col)}
          {#if col.key === "name"}
            <Button variant="link" onclick={(e) => (e.stopPropagation(), openMix(m.id))}>{m.name}</Button>
          {:else if col.key === "play_time_secs"}
            {mmss(m.play_time_secs)}
          {:else if col.key === "track_count"}
            {m.track_count}
          {:else if col.key === "date"}
            {fmtDate(dateOf(m.id))}
          {:else}
            <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
            <span onclick={(e) => e.stopPropagation()}>
              {#if listMeta.byId.has(m.id)}<Stars rating={ratingOf(m.id)} onchange={(r) => rate(m, r)} />{:else}<span class="muted">–</span>{/if}
            </span>
          {/if}
        {/snippet}
      </Table>
    {:else if !loading}
      <div class="empty">
        <p>No mixes on this device yet.</p>
        <p class="muted">Create one, or import a .pmk file made with the Tonium Editor or the Pacemaker.</p>
        <Button variant="primary" onclick={newMix}>New mix</Button>
      </div>
    {/if}
  </div>
</div>

<style>
  .list {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
  }
  .bar {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 8px;
    padding: 10px 14px;
    border-bottom: 1px solid var(--line);
  }
  h2 {
    margin: 0;
    font-size: 16px;
  }
  .chip {
    color: var(--muted);
    font-size: 12px;
    margin-right: 8px;
  }
  .sep {
    width: 1px;
    height: 20px;
    background: var(--line);
    margin: 0 4px;
  }
  .banner {
    padding: 8px 14px;
    background: var(--accent-soft);
    border-bottom: 1px solid var(--line);
    color: var(--text);
    display: flex;
    gap: 12px;
    align-items: center;
  }
  .body {
    flex: 1;
    min-height: 0;
    overflow: auto;
  }
  .body :global(tr.picked) {
    background: var(--accent-soft) !important;
  }
  .empty {
    padding: 28px 14px;
  }
  .empty p {
    margin: 0 0 8px;
  }
  .muted {
    color: var(--muted);
  }
  .bad {
    color: var(--bad);
  }
</style>
