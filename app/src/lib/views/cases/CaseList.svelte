<script lang="ts">
  import type { CaseInfo } from "$lib/api/cases";
  import { mmss } from "$lib/format";
  import { cases } from "$lib/state/cases.svelte";
  import { filters } from "$lib/state/filters.svelte";
  import { jobs } from "$lib/state/jobs.svelte";
  import { selection } from "$lib/state/selection.svelte";
  import { session } from "$lib/state/session.svelte";
  import Button from "$lib/ui/Button.svelte";
  import ContextMenu, { type MenuItem } from "$lib/ui/ContextMenu.svelte";
  import Stars from "$lib/ui/Stars.svelte";
  import Table, { type Column } from "$lib/ui/Table.svelte";
  import ConfirmDialog from "./ConfirmDialog.svelte";
  import NameDialog from "./NameDialog.svelte";

  const columns: Column[] = [
    { key: "name", label: "Name" },
    { key: "track_count", label: "Tracks", num: true },
    { key: "play_time_secs", label: "Length", num: true },
    { key: "rating", label: "Rating" },
    { key: "times_played", label: "Plays", num: true },
    { key: "actions", label: "" },
  ];

  let naming = $state<{ kind: "new" } | { kind: "from-selection" } | { kind: "rename"; c: CaseInfo } | null>(null);
  let deleting = $state<CaseInfo | null>(null);
  let menu = $state<{ x: number; y: number; c: CaseInfo } | null>(null);

  let rows = $derived.by(() => {
    const q = filters.query.trim().toLowerCase();
    return q ? cases.infos.filter((c) => [c.name, c.genre].some((f) => f.toLowerCase().includes(q))) : cases.infos;
  });
  let selected = $derived(selection.trackIds.length);
  // the device copy of a smart case, so deleting it can say so
  const smartOf = (c: CaseInfo) => cases.smart.find((s) => s.device_case_id === c.id);
  // the heart on a track looks this case up by its exact name
  const isFav = (c: CaseInfo) => c.name === "Favourites";
  const FAV_NOTE = "This is the case the heart on a track uses.";
  const othersThan = (id?: number) => cases.infos.filter((x) => x.id !== id).map((x) => x.name);

  // The shared Table has no row context-menu hook, so find the row from the event.
  function rowMenu(e: MouseEvent) {
    const tr = (e.target as HTMLElement).closest("tbody tr") as HTMLTableRowElement | null;
    const c = tr ? rows[tr.sectionRowIndex] : undefined;
    if (!c) return;
    e.preventDefault();
    menu = { x: e.clientX, y: e.clientY, c };
  }

  const menuItems = (c: CaseInfo): MenuItem[] => [
    { label: "Open", onclick: () => open(c) },
    { label: "Rename…", onclick: () => (naming = { kind: "rename", c }), disabled: !jobs.canWrite },
    { label: "Delete…", onclick: () => (deleting = c), disabled: !jobs.canWrite, danger: true },
  ];

  function open(c: CaseInfo) {
    const col = session.library?.cases.find((x) => x.id === c.id);
    if (col) cases.openCase(col);
  }

  async function confirmName(name: string) {
    const what = naming;
    naming = null;
    if (!what) return;
    if (what.kind === "rename") await cases.rename(what.c.id, name);
    else {
      const id = await cases.create(name, what.kind === "from-selection" ? [...selection.trackIds] : []);
      const col = id === undefined ? undefined : session.library?.cases.find((x) => x.id === id);
      if (col) await cases.openCase(col);
    }
  }
</script>

<div class="bar">
  <h2>Cases</h2>
  <Button disabled={!jobs.canWrite} onclick={() => (naming = { kind: "new" })}>New case</Button>
  <Button
    disabled={!jobs.canWrite || selected === 0}
    title={selected === 0 ? "Select tracks in the Tracks view first" : ""}
    onclick={() => (naming = { kind: "from-selection" })}
  >From selection{selected > 0 ? ` (${selected})` : ""}</Button>
  {#if cases.smart.some((s) => s.on_device)}
    <Button
      disabled={!jobs.canWrite}
      onclick={() => cases.refreshDeviceCopies()}
      title="Rewrite the Pacemaker copy of every smart case that is switched on, from the current library"
    >Refresh smart cases</Button>
  {/if}
  <span class="muted">Cases are the Pacemaker's own playlists; the order here is the order on the device.</span>
</div>

<div role="presentation" oncontextmenu={rowMenu}>
<Table {columns} {rows} rowKey={(c) => c.id} onrowclick={open}>
  {#snippet cell(c, col)}
    {#if col.key === "name"}
      <span>
        <Button variant="link" onclick={(e) => (e.stopPropagation(), open(c))}>{c.name}</Button>
        {#if isFav(c)}<span class="tag" title={FAV_NOTE}>hearts</span>{/if}
        {#if smartOf(c)}<span class="tag" title="Made by the smart case “{smartOf(c)?.name}”">smart</span>{/if}
      </span>
    {:else if col.key === "track_count"}
      {c.track_count}
    {:else if col.key === "play_time_secs"}
      {mmss(c.play_time_secs)}
    {:else if col.key === "rating"}
      <Stars rating={c.rating} />
    {:else if col.key === "times_played"}
      {c.times_played}
    {:else}
      <span class="actions">
        <Button small variant="ghost" disabled={!jobs.canWrite} onclick={(e) => (e.stopPropagation(), (naming = { kind: "rename", c }))}>Rename</Button>
        <Button small variant="ghost" disabled={!jobs.canWrite} onclick={(e) => (e.stopPropagation(), (deleting = c))}>Delete</Button>
      </span>
    {/if}
  {/snippet}
</Table>
</div>
{#if !cases.loaded}
  <p class="empty">Loading cases…</p>
{:else if rows.length === 0}
  <p class="empty">{filters.query ? "No cases match the search." : "No cases on this device yet. Use New case, or the heart on a track for Favourites."}</p>
{/if}

{#if menu}
  <ContextMenu x={menu.x} y={menu.y} items={menuItems(menu.c)} onclose={() => (menu = null)} />
{/if}

{#if naming}
  <NameDialog
    title={naming.kind === "rename" ? "Rename case" : naming.kind === "from-selection" ? `New case from ${selected} selected track${selected === 1 ? "" : "s"}` : "New case"}
    initial={naming.kind === "rename" ? naming.c.name : ""}
    note={naming.kind === "rename" && isFav(naming.c) ? `${FAV_NOTE} If you rename it, the next heart click starts a new, empty “Favourites” case and the hearts here stop showing.` : undefined}
    taken={othersThan(naming.kind === "rename" ? naming.c.id : undefined)}
    confirmLabel={naming.kind === "rename" ? "Rename" : "Create"}
    onconfirm={confirmName}
    onclose={() => (naming = null)}
  />
{/if}

{#if deleting}
  {@const c = deleting}
  <ConfirmDialog
    title="Delete this case?"
    message={`Delete the case “${c.name}” (${c.track_count} track${c.track_count === 1 ? "" : "s"}) from the Pacemaker? The tracks stay in the library; only the case is removed.${isFav(c) ? ` ${FAV_NOTE} Deleting it clears every heart; the next heart click starts a new, empty Favourites case.` : ""}${smartOf(c) ? ` It is the Pacemaker copy of the smart case “${smartOf(c)?.name}”, which will make it again the next time you refresh it.` : ""}`}
    confirmLabel="Delete case"
    onconfirm={() => cases.remove(c.id)}
    onclose={() => (deleting = null)}
  />
{/if}

<style>
  .bar {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 10px;
    border-bottom: 1px solid var(--line);
    flex-wrap: wrap;
  }
  .bar :global(.btn) {
    white-space: nowrap;
  }
  h2 {
    margin: 0 6px 0 0;
    font-size: 15px;
  }
  .actions {
    display: inline-flex;
    gap: 2px;
  }
  .tag {
    margin-left: 6px;
    padding: 0 6px;
    border-radius: 8px;
    font-size: 11px;
    background: var(--accent-soft);
    color: var(--accent);
  }
</style>
