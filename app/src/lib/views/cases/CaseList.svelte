<script lang="ts">
  import type { CaseInfo } from "$lib/api/cases";
  import { mmss } from "$lib/format";
  import { cases } from "$lib/state/cases.svelte";
  import { filters } from "$lib/state/filters.svelte";
  import { freshLabelFilter, labelFilterActive, matchesLabels, describeLabelFilter } from "$lib/state/labels";
  import { caseLabels, thingEditor } from "$lib/state/thingLabels.svelte";
  import { jobs } from "$lib/state/jobs.svelte";
  import { selection } from "$lib/state/selection.svelte";
  import { session } from "$lib/state/session.svelte";
  import { smartRefresh } from "$lib/state/smartRefresh.svelte";
  import { smartSettings } from "$lib/state/smartSettings.svelte";
  import { toast } from "$lib/state/toast.svelte";
  import { trackDropTarget } from "$lib/state/trackDrag.svelte";
  import Button from "$lib/ui/Button.svelte";
  import ContextMenu, { type MenuItem } from "$lib/ui/ContextMenu.svelte";
  import Stars from "$lib/ui/Stars.svelte";
  import Table, { type Column } from "$lib/ui/Table.svelte";
  import { caseExport } from "$lib/state/caseExport.svelte";
  import { defaultSelection } from "$lib/state/exportLogic";
  import { copyName, plural } from "./caseLogic";
  import ConfirmDialog from "./ConfirmDialog.svelte";
  import { exportMenuItems } from "./exportMenu";
  import ThingChips from "../metadata/ThingChips.svelte";
  import ThingFilterBar from "../metadata/ThingFilterBar.svelte";
  import MergeDialog from "./MergeDialog.svelte";
  import NameDialog from "./NameDialog.svelte";
  import SmartSettingsDialog from "./SmartSettingsDialog.svelte";
  import { remakeWhen } from "./smartRefreshLogic";

  const columns: Column[] = [
    { key: "pick", label: "" },
    { key: "name", label: "Name" },
    { key: "track_count", label: "Tracks", num: true },
    { key: "play_time_secs", label: "Length", num: true },
    { key: "rating", label: "Rating" },
    { key: "times_played", label: "Plays", num: true },
    { key: "actions", label: "" },
  ];
  const SORTABLE = new Set(["name", "track_count", "play_time_secs", "rating", "times_played"]);

  let naming = $state<{ kind: "new" } | { kind: "from-selection" } | { kind: "rename"; c: CaseInfo } | { kind: "duplicate"; c: CaseInfo } | null>(null);
  let deleting = $state<CaseInfo[] | null>(null);
  let merging = $state<number[] | null>(null);
  let settingsOpen = $state(false);
  $effect(() => {
    void smartSettings.ensure(); // the delete notes say when a deleted smart-case copy comes back
  });
  let menu = $state<{ x: number; y: number; c: CaseInfo } | null>(null);
  // the cases ticked in the first column (bulk delete, merge)
  let picked = $state<Set<number>>(new Set());
  let sortKey = $state<string>("name");
  let sortAsc = $state(true);
  let dropId = $state<number | null>(null);
  // colour and tag conditions (app-side labels of cases); cases with no labels never match one
  let labelFilter = $state(freshLabelFilter());

  const collator = new Intl.Collator(undefined, { sensitivity: "base", numeric: true });
  let rows = $derived.by(() => {
    const q = filters.query.trim().toLowerCase();
    const named = q ? cases.infos.filter((c) => [c.name, c.genre].some((f) => f.toLowerCase().includes(q))) : cases.infos;
    const list = labelFilterActive(labelFilter) ? named.filter((c) => matchesLabels(caseLabels.get(c.id), labelFilter)) : named;
    const dir = sortAsc ? 1 : -1;
    const k = sortKey as keyof CaseInfo;
    return [...list].sort((a, b) => (k === "name" ? collator.compare(a.name, b.name) : (a[k] as number) - (b[k] as number)) * dir || collator.compare(a.name, b.name) || a.id - b.id);
  });
  let pickedCases = $derived(cases.infos.filter((c) => picked.has(c.id)));
  // labels go to the ticked cases that are on screen, so a search or filter never hides a case the edit touches
  let pickedShown = $derived(rows.filter((c) => picked.has(c.id)));
  let selected = $derived(selection.trackIds.length);
  // the device copy of a smart case, so deleting it can say so
  const smartOf = (c: CaseInfo) => cases.smart.find((s) => s.device_case_id === c.id);
  // the heart on a track looks this case up by its exact name
  const isFav = (c: CaseInfo) => c.name === "Favourites";
  const FAV_NOTE = "This is the case the heart on a track uses.";
  const othersThan = (id?: number) => cases.infos.filter((x) => x.id !== id).map((x) => x.name);
  const tracksWord = (n: number) => plural(n, "track");

  // a case that was deleted or merged away leaves the ticks
  $effect(() => {
    const live = new Set(cases.infos.map((c) => c.id));
    if ([...picked].some((id) => !live.has(id))) picked = new Set([...picked].filter((id) => live.has(id)));
  });

  function sortBy(key: string) {
    if (!SORTABLE.has(key)) return;
    if (sortKey === key) sortAsc = !sortAsc;
    else {
      sortKey = key;
      sortAsc = true;
    }
  }

  // a tag on a row narrows the list to it, as if it had been picked in the filter
  function filterByTag(tag: string) {
    labelFilter = { ...labelFilter, tags: [tag], tagMode: "any" };
  }

  function toggle(id: number) {
    const next = new Set(picked);
    if (!next.delete(id)) next.add(id);
    picked = next;
  }

  // The shared Table has no row hooks, so find the row from the event.
  const rowAt = (el: Element) => {
    const tr = el.closest("tbody tr") as HTMLTableRowElement | null;
    return tr ? rows[tr.sectionRowIndex] : undefined;
  };
  const rowOf = (e: Event) => rowAt(e.target as HTMLElement);

  function rowMenu(e: MouseEvent) {
    const c = rowOf(e);
    if (!c) return;
    e.preventDefault();
    menu = { x: e.clientX, y: e.clientY, c };
  }

  // Tracks dragged from a table (state/trackDrag.svelte.ts) land on the case row under the pointer.
  const dropTarget = (hit: Element) => {
    const c = rowAt(hit);
    return c && jobs.canWrite && !smartOf(c) ? c : undefined;
  };
  const dropOptions = {
    accept: (hit: Element) => !!dropTarget(hit),
    onhover: (hit: Element | null) => (dropId = (hit && dropTarget(hit)?.id) ?? null),
    ondrop: (ids: number[], hit: Element) => {
      const c = dropTarget(hit);
      if (c && ids.length > 0) void cases.dropTracks(c.id, c.name, ids);
    },
  };

  const exportItems = (c: CaseInfo): MenuItem[] =>
    exportMenuItems({
      run: (f) => cases.exportOne(c, f),
      options: () => caseExport.show({ caseIds: [c.id], smartIds: [] }),
      disabled: c.track_count === 0,
      labels: { m3u8: "Export as M3U8…", csv: "Export as CSV…", rekordbox: "Export as rekordbox XML…" },
    });

  // the ticked cases, or (nothing ticked) every case and smart case; the dialog shows what it will write
  function exportMany() {
    if (picked.size > 0) void caseExport.show({ caseIds: [...picked], smartIds: [] });
    else void caseExport.show(defaultSelection(cases.infos, cases.smart));
  }

  const menuItems = (c: CaseInfo): MenuItem[] => [
    { label: "Open", onclick: () => open(c) },
    { label: picked.has(c.id) && pickedShown.length > 1 ? `Tags and colour of ${pickedShown.length} ticked cases…` : "Tags and colour…", onclick: () => thingEditor.edit("case", picked.has(c.id) && pickedShown.length > 1 ? pickedShown.map((x) => x.id) : [c.id]) },
    { label: "Rename…", onclick: () => (naming = { kind: "rename", c }), disabled: !jobs.canWrite },
    { label: "Duplicate…", onclick: () => (naming = { kind: "duplicate", c }), disabled: !jobs.canWrite },
    { label: "Merge with other cases…", onclick: () => (merging = [...new Set([...picked, c.id])]), disabled: !jobs.canWrite || cases.infos.length < 2 },
    ...exportItems(c),
    { label: picked.has(c.id) && picked.size > 1 ? `Delete ${picked.size} ticked cases…` : "Delete…", onclick: () => (deleting = picked.has(c.id) && picked.size > 1 ? pickedCases : [c]), disabled: !jobs.canWrite, danger: true },
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
    else if (what.kind === "duplicate") {
      const id = await cases.duplicate(what.c.id, name);
      if (id !== undefined) toast.show(`Made “${name}” with ${tracksWord(cases.infos.find((x) => x.id === id)?.track_count ?? 0)}`, "ok");
    } else {
      const id = await cases.create(name, what.kind === "from-selection" ? [...selection.trackIds] : []);
      const col = id === undefined ? undefined : session.library?.cases.find((x) => x.id === id);
      if (col) await cases.openCase(col);
    }
  }

  // Deleting one or several: one confirmation naming every case, one transaction.
  function deleteNote(list: CaseInfo[]): string {
    const notes: string[] = [];
    if (list.some(isFav)) notes.push(`“Favourites” is the case the heart on a track uses: deleting it clears every heart, and the next heart click starts a new, empty Favourites case.`);
    const smart = list.filter(smartOf);
    if (smart.length > 0) notes.push(`${smart.map((c) => `“${c.name}”`).join(", ")} ${smart.length === 1 ? "is the Pacemaker copy of a smart case, which will make it" : "are Pacemaker copies of smart cases, which will make them"} again ${remakeWhen(smartSettings.value.refresh_mode)}.`);
    const used = cases.smartCasesUsing(list.map((c) => c.id));
    if (used.length > 0) notes.push(`${used.map((n) => `“${n}”`).join(", ")} ${used.length === 1 ? "is a smart case that has" : "are smart cases that have"} a rule about ${list.length === 1 ? "this case" : "one of these cases"}; ${used.length === 1 ? "it" : "they"} will report an error until you pick another case in the rule.`);
    return notes.join(" ");
  }

  async function confirmDelete(list: CaseInfo[]) {
    const done = await cases.removeMany(list.map((c) => ({ id: c.id, name: c.name, date_created: c.date_created })));
    if (done !== undefined) {
      picked = new Set();
      toast.show(`Deleted ${plural(done.deleted, "case")}`, "ok");
      // a case that changed while the dialog was open is not deleted: name each one
      if (done.kept.length > 0) session.error = done.kept.map((k) => `“${k.name}”: ${k.error}`).join("; ");
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
  <Button disabled={!jobs.canWrite || cases.infos.length < 2} onclick={() => (merging = [...picked])} title="Merge cases into a new or an existing case, without repeats">
    Merge{picked.size > 1 ? ` ${picked.size}` : ""}…
  </Button>
  <Button disabled={!jobs.canWrite || picked.size === 0} onclick={() => (deleting = pickedCases)}>Delete{picked.size > 0 ? ` ${picked.size}` : ""}…</Button>
  <Button disabled={pickedShown.length === 0} onclick={() => thingEditor.edit("case", pickedShown.map((c) => c.id))} title="Tags and a colour label for the ticked cases. Kept by this app only; nothing is written to the device.">Tags and colour{pickedShown.length > 0 ? ` (${pickedShown.length})` : ""}…</Button>
  {#if cases.smart.some((s) => s.on_device)}
    <Button
      disabled={!jobs.canWrite || smartRefresh.checking || smartRefresh.applying}
      onclick={() => smartRefresh.review()}
      title="Show what rewriting the Pacemaker copy of every smart case that is switched on would change, then do it if you agree"
    >Refresh smart cases…</Button>
  {/if}
  <Button
    disabled={cases.infos.length === 0 && cases.smart.length === 0}
    onclick={exportMany}
    title="Save several cases as playlist files in a folder, one file per case, with the audio copied alongside if you like"
  >Export{picked.size > 0 ? ` ${picked.size} ticked` : " all"}…</Button>
  <Button onclick={() => (settingsOpen = true)} title="Whether the Pacemaker copies of smart cases refresh by themselves, and how smart cases are exported">Smart case settings…</Button>
  <span class="muted">Cases are the Pacemaker's own playlists; the order here is the order on the device.</span>
</div>
{#if cases.infos.length > 0}
  <div class="pickbar filterbar">
    <span class="muted">Filter by label</span>
    <ThingFilterBar bind:filter={labelFilter} vocabulary={caseLabels.vocabulary} thing="case" />
    {#if labelFilterActive(labelFilter)}<Button small variant="link" onclick={() => (labelFilter = freshLabelFilter())}>Clear</Button>{/if}
  </div>
{/if}
{#if rows.length > 0}
  <div class="pickbar muted">
    {#if picked.size > 0}{plural(picked.size, "case")} ticked ·{/if}
    <Button small variant="link" onclick={() => (picked = new Set(rows.map((c) => c.id)))}>Tick all {rows.length === cases.infos.length ? "" : "shown "}</Button>
    {#if picked.size > 0}<Button small variant="link" onclick={() => (picked = new Set())}>Clear ticks</Button>{/if}
    · Drag tracks from the Tracks view onto a case to add them, or use Add to case… there (not onto a smart case's copy: its rules rewrite it)
  </div>
{/if}

<div class="wrap" role="presentation" oncontextmenu={rowMenu} use:trackDropTarget={dropOptions}>
  <Table {columns} {rows} rowKey={(c) => c.id} onrowclick={open} {sortKey} {sortAsc} onsort={sortBy} rowClass={(c) => (c.id === dropId ? "drop" : "")}>
    {#snippet cell(c, col)}
      {#if col.key === "pick"}
        <input
          type="checkbox"
          checked={picked.has(c.id)}
          aria-label={`Tick ${c.name}`}
          onclick={(e) => (e.stopPropagation(), toggle(c.id))}
        />
      {:else if col.key === "name"}
        <span>
          <Button variant="link" onclick={(e) => (e.stopPropagation(), open(c))}>{c.name}</Button>
          {#if isFav(c)}<span class="tag" title={FAV_NOTE}>hearts</span>{/if}
          {#if smartOf(c)}<span class="tag" title="Made by the smart case “{smartOf(c)?.name}”">smart</span>{/if}
          <ThingChips labels={caseLabels.get(c.id)} ontag={filterByTag} />
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
          <Button small variant="ghost" disabled={!jobs.canWrite} onclick={(e) => (e.stopPropagation(), (naming = { kind: "duplicate", c }))}>Duplicate</Button>
          <Button small variant="ghost" disabled={!jobs.canWrite} onclick={(e) => (e.stopPropagation(), (deleting = [c]))}>Delete</Button>
        </span>
      {/if}
    {/snippet}
  </Table>
</div>
{#if !cases.loaded}
  <p class="empty">Loading cases…</p>
{:else if rows.length === 0}
  <p class="empty">{filters.query || labelFilterActive(labelFilter) ? `No cases match${filters.query ? " the search" : ""}${filters.query && labelFilterActive(labelFilter) ? " and" : ""}${labelFilterActive(labelFilter) ? ` the label filter (${describeLabelFilter(labelFilter)})` : ""}.` : "No cases on this device yet. Use New case, or the heart on a track for Favourites."}</p>
{/if}

{#if menu}
  <ContextMenu x={menu.x} y={menu.y} items={menuItems(menu.c)} onclose={() => (menu = null)} />
{/if}

{#if naming}
  <NameDialog
    title={naming.kind === "rename"
      ? "Rename case"
      : naming.kind === "duplicate"
        ? "Duplicate case"
        : naming.kind === "from-selection"
          ? `New case from ${selected} selected track${selected === 1 ? "" : "s"}`
          : "New case"}
    initial={naming.kind === "rename" ? naming.c.name : naming.kind === "duplicate" ? copyName(naming.c.name, othersThan()) : ""}
    note={naming.kind === "rename" && isFav(naming.c)
      ? `${FAV_NOTE} If you rename it, the next heart click starts a new, empty “Favourites” case and the hearts here stop showing.`
      : naming.kind === "duplicate"
        ? `Makes a new case with the same ${tracksWord(naming.c.track_count)} in the same order and the same genre, year, rating and notes. “${naming.c.name}” is not changed.`
        : undefined}
    taken={othersThan(naming.kind === "rename" ? naming.c.id : undefined)}
    confirmLabel={naming.kind === "rename" ? "Rename" : naming.kind === "duplicate" ? "Duplicate" : "Create"}
    onconfirm={confirmName}
    onclose={() => (naming = null)}
  />
{/if}

{#if settingsOpen}
  <SmartSettingsDialog onclose={() => (settingsOpen = false)} />
{/if}

{#if merging}
  <MergeDialog initial={merging} onclose={() => (merging = null)} />
{/if}

{#if deleting}
  {@const list = deleting}
  {@const total = list.reduce((s, c) => s + c.track_count, 0)}
  <ConfirmDialog
    title={list.length === 1 ? "Delete this case?" : `Delete ${list.length} cases?`}
    message={`Delete ${list.length === 1 ? `the case “${list[0].name}”` : `these ${list.length} cases`} (${tracksWord(total)} in all) from the Pacemaker? The tracks stay in the library; only the ${list.length === 1 ? "case is" : "cases are"} removed. This cannot be undone.${deleteNote(list) ? ` ${deleteNote(list)}` : ""}`}
    items={list.map((c) => `${c.name} (${tracksWord(c.track_count)})`)}
    confirmLabel={list.length === 1 ? "Delete case" : `Delete ${list.length} cases`}
    onconfirm={() => confirmDelete(list)}
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
  .pickbar.filterbar {
    gap: 10px;
  }
  .pickbar {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 4px 10px;
    border-bottom: 1px solid var(--line);
    font-size: 12px;
  }
  h2 {
    margin: 0 6px 0 0;
    font-size: 15px;
  }
  .wrap :global(tr.drop) {
    outline: 2px solid var(--accent);
    outline-offset: -2px;
    background: var(--accent-soft);
  }
  .wrap :global(input[type="checkbox"]) {
    padding: 0;
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
