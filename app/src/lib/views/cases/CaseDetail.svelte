<script lang="ts">
  import { onDestroy, untrack } from "svelte";
  import type { CaseMeta } from "$lib/api/cases";
  import { mmss } from "$lib/format";
  import { caseExport } from "$lib/state/caseExport.svelte";
  import { cases } from "$lib/state/cases.svelte";
  import { filters } from "$lib/state/filters.svelte";
  import { jobs } from "$lib/state/jobs.svelte";
  import { metadataEditor } from "$lib/state/metadata.svelte";
  import { nav } from "$lib/state/nav.svelte";
  import { playback } from "$lib/state/playback.svelte";
  import { selection } from "$lib/state/selection.svelte";
  import { toast } from "$lib/state/toast.svelte";
  import { session } from "$lib/state/session.svelte";
  import { insertIndex } from "$lib/state/dragLogic";
  import { armReorder, endReorder } from "$lib/state/pointerReorder.svelte";
  import { armTrackDrag, endTrackDrag, trackDrag, trackDropTarget } from "$lib/state/trackDrag.svelte";
  import Button from "$lib/ui/Button.svelte";
  import ContextMenu, { type MenuItem } from "$lib/ui/ContextMenu.svelte";
  import Stars from "$lib/ui/Stars.svelte";
  // Read-only reuse of the Tracks view's pure sort / key logic and its small waveform, so a case
  // reads and sorts like the library.
  import MiniWave from "../tracks/MiniWave.svelte";
  import ThingChips from "../metadata/ThingChips.svelte";
  import { caseLabels, thingEditor } from "$lib/state/thingLabels.svelte";
  import { keyColour, parseKey, sortTracks } from "../tracks/trackLogic";
  import { setlist } from "../setlist/setlist.svelte";
  import { vdj } from "../vdj/vdj.svelte";
  import CaseTrackPicker from "./CaseTrackPicker.svelte";
  import { CASE_COLUMNS, type CaseCol, type Row, copyName, plural, searchRows, sortRows, trackLabel } from "./caseLogic";
  import { caseView } from "./caseView.svelte";
  import ConfirmDialog from "./ConfirmDialog.svelte";
  import { exportMenuItems } from "./exportMenu";
  import MergeDialog from "./MergeDialog.svelte";
  import NameDialog from "./NameDialog.svelte";
  import TransferDialog from "./TransferDialog.svelte";

  // The view only mounts this with a case open; the fallback covers the moment it closes.
  let open = $derived(cases.open ?? { id: -1, name: "", play_time_secs: 0, track_count: 0 });
  let info = $derived(cases.openInfo);

  onDestroy(() => {
    endReorder();
    endTrackDrag();
  });

  // ---- rows -------------------------------------------------------------------------------
  let byId = $derived(new Map((session.library?.tracks ?? []).map((t) => [t.track_id, t])));
  // a track can sit twice in casetracks (the table has no unique constraint); show it once
  let all = $derived<Row[]>([...new Set(cases.trackIds)].map((id) => ({ id, t: byId.get(id) })));
  let position = $derived(new Map(all.map((r, i) => [r.id, i + 1])));
  let missing = $derived(all.filter((r) => !r.t));
  let query = $derived(filters.query.trim().toLowerCase());
  // the case's own order, or a view-only sort (the same sort the Tracks view uses)
  let viewSorted = $derived(caseView.prefs.sortKey !== "order" || !caseView.prefs.sortAsc);
  let rows = $derived(
    sortRows(searchRows(all, query), caseView.prefs.sortKey, caseView.prefs.sortAsc, sortTracks, (id) => cases.favourites.has(id)),
  );
  let totalSecs = $derived(all.reduce((s, r) => s + (r.t?.play_time_secs ?? 0), 0));
  let selected = $derived(new Set(selection.trackIds));
  let selectedRows = $derived(all.filter((r) => selected.has(r.id)));
  let cols = $derived(caseView.shown);
  // dragging to reorder needs the whole list on screen, in the case's order, every row a real track
  let canReorder = $derived(jobs.canWrite && !query && !viewSorted && missing.length === 0);

  const label = (r: Row) => trackLabel(r.id, r.t);
  const sortLabel = $derived(CASE_COLUMNS.find((c) => c.sort === caseView.prefs.sortKey)?.label || (caseView.prefs.sortKey === "fav" ? "favourites" : ""));

  // ---- selection --------------------------------------------------------------------------
  let anchor = $state<number | null>(null);

  // Roving tab stop: one row is in the tab order, the arrow keys move it.
  let focusIdx = $state(0);
  let tbody = $state<HTMLElement>();

  function rowKey(e: KeyboardEvent, i: number) {
    if (e.altKey || e.ctrlKey || e.metaKey || (e.target as HTMLElement).closest("button")) return;
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      const n = Math.max(0, Math.min(rows.length - 1, i + (e.key === "ArrowDown" ? 1 : -1)));
      if (e.shiftKey && anchor !== null && anchor < rows.length) {
        const [a, b] = [Math.min(anchor, n), Math.max(anchor, n)];
        selection.trackIds = rows.slice(a, b + 1).map((r) => r.id);
      } else {
        selection.trackIds = [rows[n].id];
        anchor = n;
      }
      focusIdx = n;
      (tbody?.children[n] as HTMLElement | undefined)?.focus();
    } else if (e.key === " ") {
      e.preventDefault();
      const id = rows[i].id;
      selection.trackIds = selected.has(id) ? selection.trackIds.filter((x) => x !== id) : [...selection.trackIds, id];
      anchor = i;
    } else if (e.key === "Enter" && rows[i].t) {
      e.preventDefault();
      play(rows[i].id);
    }
  }

  function pick(i: number, e: MouseEvent) {
    focusIdx = i;
    const id = rows[i].id;
    if (e.shiftKey && anchor !== null && anchor < rows.length) {
      const [a, b] = [Math.min(anchor, i), Math.max(anchor, i)];
      selection.trackIds = rows.slice(a, b + 1).map((r) => r.id);
    } else if (e.ctrlKey || e.metaKey) {
      selection.trackIds = selected.has(id) ? selection.trackIds.filter((x) => x !== id) : [...selection.trackIds, id];
      anchor = i;
    } else {
      selection.trackIds = [id];
      anchor = i;
    }
  }

  // ---- reorder ----------------------------------------------------------------------------
  /** Moves `moving` (kept in their current relative order) so they sit before original index `at`. */
  function moveBlock(moving: Set<number>, at: number) {
    // Work on the rows as shown (a track repeated in the case is listed once), so row indices
    // line up; the de-duplicated order is what gets written back.
    const list = all.map((r) => r.id);
    const block = list.filter((id) => moving.has(id));
    const rest = list.filter((id) => !moving.has(id));
    const into = list.slice(0, at).filter((id) => !moving.has(id)).length;
    const next = [...rest.slice(0, into), ...block, ...rest.slice(into)];
    if (next.every((id, i) => id === list[i]) && list.length === cases.trackIds.length) return;
    cases.reorder(next);
  }

  let drag = $state<{ ids: Set<number>; over: number; after: boolean } | null>(null);

  // Tracks dragged in from a table (see state/trackDrag.svelte.ts) are appended to this case. A
  // drag that started on a row of this very table is never dropped back here.
  let dropping = $state(false);
  const dropOptions = {
    accept: () => jobs.canWrite && trackDrag.from !== `case:${open.id}`,
    onhover: (hit: Element | null) => (dropping = !!hit),
    ondrop: (ids: number[]) => {
      if (ids.length > 0) void cases.dropTracks(open.id, open.name, ids);
    },
  };

  // A drag from a row carries the selected tracks of THIS case (the selection is shared with the
  // Tracks view and can hold tracks that are not here), or just the row's own track. Read when
  // the drag starts, after the press has made its selection.
  function rowDrag(i: number) {
    const r = rows[i];
    if (!r?.t) return { ids: [] };
    if (!selected.has(r.id)) {
      selection.trackIds = [r.id];
      anchor = i;
      return { ids: [r.id] };
    }
    return { ids: all.filter((x) => selected.has(x.id) && x.t).map((x) => x.id) };
  }

  // Reordering inside the table is a pointer drag from the grip cell (state/pointerReorder.svelte.ts;
  // HTML5 drag and drop does not reach the page in the Windows app); dragging a row anywhere else
  // carries tracks to another case (state/trackDrag.svelte.ts).
  function armDrag(e: PointerEvent, i: number) {
    if (!canReorder) return;
    const id = rows[i].id;
    armReorder(e, {
      rows: () => [...(tbody?.querySelectorAll<HTMLElement>("tr") ?? [])],
      scroller: () => tbody?.closest<HTMLElement>(".scroll"),
      begin: () => {
        if (!selected.has(id)) {
          selection.trackIds = [id];
          anchor = i;
        }
        const ids = new Set(all.filter((r) => r.id === id || selected.has(r.id)).map((r) => r.id));
        drag = { ids, over: i, after: false };
      },
      over: (p) => {
        if (drag) drag = { ...drag, ...p };
      },
      drop: (p) => {
        if (drag) moveBlock(drag.ids, insertIndex(p));
      },
      end: () => (drag = null),
    });
  }

  function nudge(dir: -1 | 1) {
    if (!canReorder || selectedRows.length === 0) return;
    const idx = selectedRows.map((r) => all.indexOf(r));
    moveBlock(new Set(selectedRows.map((r) => r.id)), dir < 0 ? Math.max(0, Math.min(...idx) - 1) : Math.max(...idx) + 2);
  }

  // ---- actions ----------------------------------------------------------------------------
  let adding = $state(false);
  let renaming = $state(false);
  let deleting = $state(false);
  let removing = $state<Row[] | null>(null);
  let duplicating = $state(false);
  let merging = $state(false);
  let transfer = $state<{ mode: "copy" | "move"; ids: number[] } | null>(null);
  let savingOrder = $state(false);
  let chooser = $state(false);
  let menu = $state<{ x: number; y: number; items: MenuItem[] } | null>(null);

  const dialogOpen = $derived(adding || renaming || deleting || !!removing || duplicating || merging || !!transfer || savingOrder);
  const inLibrary = (ids: number[]) => ids.filter((id) => byId.has(id));

  function play(id?: number) {
    const target = id ?? selectedRows.find((r) => r.t)?.id ?? all.find((r) => r.t)?.id;
    if (target !== undefined) playback.load(target, true);
  }

  function onkey(e: KeyboardEvent) {
    // Not while a dialog is up: its keys must not act on the case behind it.
    if (dialogOpen) return;
    if ((e.target as HTMLElement).closest("input, select, textarea, [role='dialog']")) return;
    if (e.key === "Delete" && jobs.canWrite && selectedRows.length > 0) removing = selectedRows;
    else if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "a") {
      e.preventDefault();
      selection.trackIds = rows.map((r) => r.id);
    } else if (e.altKey && e.key === "ArrowUp") (e.preventDefault(), nudge(-1));
    else if (e.altKey && e.key === "ArrowDown") (e.preventDefault(), nudge(1));
  }

  function rowMenu(e: MouseEvent, i: number) {
    e.preventDefault();
    e.stopPropagation(); // the menu closes on window events; this one must not reach it
    focusIdx = i;
    const id = rows[i].id;
    if (!selected.has(id)) {
      selection.trackIds = [id];
      anchor = i;
    }
    const picked = all.filter((r) => selection.trackIds.includes(r.id));
    const live = picked.filter((r) => r.t);
    const n = picked.length;
    const w = jobs.canWrite;
    menu = {
      x: Math.max(0, Math.min(e.clientX, innerWidth - 230)),
      y: Math.max(0, Math.min(e.clientY, innerHeight - 220)),
      items: [
        { label: "Play", onclick: () => play(live[0]?.id), disabled: live.length === 0 },
        { label: live.length === 1 ? "Edit metadata…" : `Edit metadata of ${live.length} tracks…`, onclick: () => metadataEditor.edit(live.map((r) => r.id)), disabled: live.length === 0 },
        { label: n === 1 ? "Copy to another case…" : `Copy ${n} tracks to another case…`, onclick: () => (transfer = { mode: "copy", ids: live.map((r) => r.id) }), disabled: !w || live.length === 0 },
        { label: n === 1 ? "Move to another case…" : `Move ${n} tracks to another case…`, onclick: () => (transfer = { mode: "move", ids: live.map((r) => r.id) }), disabled: !w || live.length === 0 },
        { label: n === 1 ? "Remove from this case…" : `Remove ${n} tracks from this case…`, onclick: () => (removing = picked), disabled: !w, danger: true },
      ],
    };
  }

  function exportMenu(e: MouseEvent) {
    e.stopPropagation();
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const none = all.every((x) => !x.t);
    menu = {
      x: r.left,
      y: r.bottom + 2,
      items: exportMenuItems({ run: (f) => cases.exportOne(open, f), options: () => caseExport.show({ caseIds: [open.id], smartIds: [] }), disabled: none }),
    };
  }

  const defaultCopyName = $derived(copyName(open.name, cases.infos.map((c) => c.name)));

  // The view-only sort can be written back as the case's real order (the order on the Pacemaker).
  const orderIds = $derived(rows.map((r) => r.id));
  const canSaveOrder = $derived(jobs.canWrite && viewSorted && !query && missing.length === 0);

  function sortHeader(c: CaseCol) {
    if (c.sort) caseView.sortBy(c.sort);
  }

  const ariaSort = (c: CaseCol) => (c.sort && c.sort === caseView.prefs.sortKey ? (caseView.prefs.sortAsc ? "ascending" : "descending") : "none");

  // ---- metadata panel ---------------------------------------------------------------------
  const metaOf = (c: CaseMeta): CaseMeta => ({ name: c.name, genre: c.genre, year: c.year, rating: c.rating, producer: c.producer, remixer: c.remixer, comments: c.comments });
  const same = (a: CaseMeta, b: CaseMeta) => (Object.keys(a) as (keyof CaseMeta)[]).every((k) => a[k] === b[k]);

  let draft = $state<CaseMeta>({ name: "", genre: "", year: 0, rating: 0, producer: "", remixer: "", comments: "" });
  let baseline = $state<CaseMeta | null>(null);
  let seeded = -1;

  // Re-seed the form when another case opens or the stored values change, unless it has unsaved edits.
  $effect(() => {
    const c = info;
    if (!c) return;
    const m = metaOf(c);
    untrack(() => {
      const dirty = baseline !== null && seeded === c.id && !same(draft, baseline);
      if (!dirty) draft = { ...m };
      baseline = m;
      seeded = c.id;
    });
  });

  let dirty = $derived(baseline !== null && !same({ ...draft, year: Number(draft.year) || 0 }, baseline));
  let nameOk = $derived(draft.name.trim().length > 0);

  async function save() {
    if (!info || !nameOk) return;
    await cases.saveMeta(info.id, { ...draft, name: draft.name.trim(), year: Math.max(0, Math.min(9999, Math.trunc(Number(draft.year) || 0))) });
  }

  const when = (ms: number) => (ms > 0 ? new Date(ms).toLocaleDateString(undefined, { day: "numeric", month: "short", year: "numeric" }) : "never");
</script>

<svelte:window onkeydown={onkey} onclick={() => (chooser = false)} />

<div class="split">
  <div class="main">
    <div class="crumb">
      <Button small onclick={() => cases.close()}>← Cases</Button>
      <b>{open.name}</b>
      <span class="muted">{all.length} track{all.length === 1 ? "" : "s"} · {mmss(totalSecs)}</span>
    </div>

    <div class="toolbar">
      <Button variant="primary" onclick={() => play()} disabled={all.every((r) => !r.t)}>▶ Play</Button>
      <Button onclick={() => (adding = true)} disabled={!jobs.canWrite}>Add tracks…</Button>
      <Button onclick={() => (removing = selectedRows)} disabled={!jobs.canWrite || selectedRows.length === 0}>
        Remove{selectedRows.length > 0 ? ` ${selectedRows.length}` : ""}…
      </Button>
      <Button onclick={() => nudge(-1)} disabled={!canReorder || selectedRows.length === 0} title="Move up (Alt+Up)">▲</Button>
      <Button onclick={() => nudge(1)} disabled={!canReorder || selectedRows.length === 0} title="Move down (Alt+Down)">▼</Button>
      <Button onclick={() => nav.go("setbuilder", { "setbuilder.caseId": open.id })} disabled={all.filter((r) => r.t).length < 2} title="Order this case into a set by key and tempo">Build a set…</Button>
      <Button
        onclick={() => (transfer = { mode: "copy", ids: inLibrary(selectedRows.map((r) => r.id)) })}
        disabled={!jobs.canWrite || inLibrary(selectedRows.map((r) => r.id)).length === 0}
        title="Copy the selected tracks into another case"
      >Copy to…</Button>
      <Button
        onclick={() => (transfer = { mode: "move", ids: inLibrary(selectedRows.map((r) => r.id)) })}
        disabled={!jobs.canWrite || inLibrary(selectedRows.map((r) => r.id)).length === 0}
        title="Move the selected tracks into another case (they leave this one)"
      >Move to…</Button>
      <Button
        onclick={() => metadataEditor.edit(selectedRows.filter((r) => r.t).map((r) => r.id))}
        disabled={selectedRows.every((r) => !r.t)}
      >Edit tracks…</Button>
      <Button onclick={exportMenu} disabled={all.every((r) => !r.t)} title="Save this case as a playlist or list for other software">Export this case ▾</Button>
      <span class="chooser" role="presentation" onclick={(e) => e.stopPropagation()}>
        <Button onclick={() => (chooser = !chooser)} active={chooser} aria-expanded={chooser}>Columns ▾</Button>
        {#if chooser}
          <div class="pop" role="group" aria-label="Columns">
            {#each CASE_COLUMNS.filter((c) => !c.fixed) as c (c.key)}
              <label><input type="checkbox" checked={caseView.prefs.visible[c.key]} onchange={() => caseView.toggleColumn(c.key)} /> {c.label}</label>
            {/each}
          </div>
        {/if}
      </span>
      <Button onclick={() => setlist.openCase(open.id)} disabled={all.length === 0} title="A printable setlist of this case">Setlist…</Button>
      <Button onclick={() => vdj.openCase(open.id, open.name)} disabled={all.length === 0} title="Export this case as a VirtualDJ list, with copies of the audio if you want them">VirtualDJ…</Button>
    </div>

    {#if viewSorted}
      <div class="notice">
        Showing the tracks sorted by {sortLabel || "a column"}{caseView.prefs.sortAsc ? "" : " (descending)"}; the case itself is unchanged and reordering is off.
        <Button small onclick={() => caseView.useCaseOrder()}>Back to case order</Button>
        <Button small onclick={() => (savingOrder = true)} disabled={!canSaveOrder} title={canSaveOrder ? "Make this the case's order on the Pacemaker" : "Needs the whole case on screen with every track in the library"}>Save this order to the case…</Button>
      </div>
    {/if}

    {#if missing.length > 0}
      <div class="warn">
        {missing.length} track{missing.length === 1 ? " is" : "s are"} no longer in the library.
        <Button small onclick={() => (removing = missing)} disabled={!jobs.canWrite}>Remove {missing.length === 1 ? "it" : "them"}…</Button>
        <span class="muted">Reordering is off until they are removed.</span>
      </div>
    {/if}

    <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
    <div
      class="scroll"
      class:dropping
      tabindex="0"
      role="region"
      aria-label="Tracks in the case"
      use:trackDropTarget={dropOptions}
    >
      <table role="grid" aria-multiselectable="true" aria-label="Tracks in the case">
        <thead>
          <tr>
            <th
              class="grip sortable"
              tabindex="0"
              aria-sort={caseView.prefs.sortKey === "order" ? (caseView.prefs.sortAsc ? "ascending" : "descending") : "none"}
              title="Position in the case; click to sort by it"
              onclick={() => caseView.sortBy("order")}
              onkeydown={(e) => (e.key === "Enter" || e.key === " ") && (e.preventDefault(), caseView.sortBy("order"))}
            >#{caseView.prefs.sortKey === "order" && !caseView.prefs.sortAsc ? " ▼" : ""}</th>
            {#each cols as c (c.key)}
              <th
                class:num={c.num}
                class:heart={c.key === "fav"}
                class:sortable={!!c.sort}
                tabindex={c.sort ? 0 : -1}
                aria-sort={ariaSort(c)}
                onclick={() => sortHeader(c)}
                onkeydown={(e) => c.sort && (e.key === "Enter" || e.key === " ") && (e.preventDefault(), sortHeader(c))}
              >
                {#if c.key === "fav"}<span title="Favourites">♥</span>{:else}{c.label}{/if}{c.sort && c.sort === caseView.prefs.sortKey ? (caseView.prefs.sortAsc ? " ▲" : " ▼") : ""}
              </th>
            {/each}
          </tr>
        </thead>
        <tbody bind:this={tbody}>
          {#each rows as r, i (r.id)}
            <tr
              tabindex={i === Math.min(focusIdx, rows.length - 1) ? 0 : -1}
              aria-selected={selected.has(r.id)}
              onkeydown={(e) => rowKey(e, i)}
              class:sel={selected.has(r.id)}
              class:dim={!r.t || session.needs.has(r.id)}
              class:before={drag && drag.over === i && !drag.after && !drag.ids.has(r.id)}
              class:after={drag && drag.over === i && drag.after && !drag.ids.has(r.id)}
              class:moving={drag?.ids.has(r.id)}
              class:reorder={canReorder}
              onclick={(e) => pick(i, e)}
              ondblclick={() => r.t && play(r.id)}
              oncontextmenu={(e) => rowMenu(e, i)}
              onpointerdown={(e) => armTrackDrag(e, `case:${open.id}`, () => rowDrag(i))}
            >
              <td class="grip muted" onpointerdown={(e) => armDrag(e, i)} data-no-drag={canReorder ? "" : undefined} title={canReorder ? "Drag to reorder" : undefined}>{canReorder ? "≡ " : ""}{position.get(r.id)}</td>
              {#each cols as c (c.key)}
                {#if c.key === "fav"}
                  <td class="heart">
                    {#if r.t}
                      <button
                        class="fav"
                        class:on={cases.favourites.has(r.id)}
                        aria-label={cases.favourites.has(r.id) ? "Remove from Favourites" : "Add to Favourites"}
                        aria-pressed={cases.favourites.has(r.id)}
                        disabled={!jobs.canWrite}
                        onclick={(e) => (e.stopPropagation(), cases.toggleFavourite(r.id))}
                      >♥</button>
                    {/if}
                  </td>
                {:else if !r.t}
                  {#if c.key === "title"}<td colspan={cols.length - 1}>Track #{r.id} is no longer in the library</td>{/if}
                {:else if c.key === "title"}
                  <td>{r.t.title}{#if session.needs.has(r.id)}<span class="badge" title={session.needs.get(r.id)}>needs analysis</span>{/if}</td>
                {:else if c.key === "artist"}
                  <td>{r.t.artist}</td>
                {:else if c.key === "album"}
                  <td>{r.t.album}</td>
                {:else if c.key === "genre"}
                  <td>{r.t.genre}</td>
                {:else if c.key === "wave"}
                  <td class="wave">{#if r.t.location && session.library}<MiniWave root={session.library.root} location={r.t.location} />{/if}</td>
                {:else if c.key === "bpm"}
                  <td class="num">{r.t.bpm && !session.needs.has(r.id) ? r.t.bpm.toFixed(2) : "–"}</td>
                {:else if c.key === "key"}
                  {@const k = parseKey(r.t.key)}
                  <td>
                    {#if k}
                      <span class="key" style:color={keyColour(k)} style:background="color-mix(in srgb, {keyColour(k)} 14%, transparent)" title={r.t.key}>{k.code}</span>
                    {:else}{r.t.key}{/if}
                  </td>
                {:else if c.key === "rating"}
                  <td><Stars rating={r.t.rating} /></td>
                {:else if c.key === "plays"}
                  <td class="num">{r.t.times_played || ""}</td>
                {:else if c.key === "time"}
                  <td class="num">{mmss(r.t.play_time_secs)}</td>
                {/if}
              {/each}
            </tr>
          {/each}
        </tbody>
      </table>
      {#if rows.length === 0}
        <p class="empty">
          {query ? "No tracks in this case match the search." : "This case is empty. Use Add tracks…, drag tracks onto it from the Tracks view, or select tracks in the Tracks view and make a case from them."}
        </p>
      {/if}
    </div>
    <div class="hintbar muted">
      Drag the ≡ number to reorder (in case order) · drag a row onto a case in the sidebar to copy it there · click a column to sort the view · the order is the order on the Pacemaker
    </div>
  </div>

  <aside class="insp">
    <h3>{info?.name ?? open.name}</h3>
    <div class="muted">case · {all.length} track{all.length === 1 ? "" : "s"} · {mmss(totalSecs)}</div>
    <div class="labelrow">
      {#if caseLabels.get(open.id)}<ThingChips labels={caseLabels.get(open.id)} max={8} />{:else}<span class="muted">No tags or colour</span>{/if}
      <Button small variant="link" onclick={() => thingEditor.edit("case", [open.id])} title="Tags and a colour label. Kept by this app only; nothing is written to the device.">Tags and colour…</Button>
    </div>
    {#if info}
      <div class="field"><label for="c-name">Name</label><input id="c-name" bind:value={draft.name} maxlength="255" disabled={!jobs.canWrite} /></div>
      <div class="field"><label for="c-genre">Genre</label><input id="c-genre" bind:value={draft.genre} disabled={!jobs.canWrite} /></div>
      <div class="field"><label for="c-year">Year</label><input id="c-year" type="number" min="0" max="9999" bind:value={draft.year} disabled={!jobs.canWrite} /></div>
      <div class="field">
        <span class="lbl">Rating</span>
        {#if jobs.canWrite}<Stars rating={draft.rating} onchange={(n) => (draft.rating = n)} />{:else}<Stars rating={draft.rating} />{/if}
      </div>
      <div class="field"><label for="c-prod">Producer</label><input id="c-prod" bind:value={draft.producer} disabled={!jobs.canWrite} /></div>
      <div class="field"><label for="c-rem">Remixer</label><input id="c-rem" bind:value={draft.remixer} disabled={!jobs.canWrite} /></div>
      <div class="field"><label for="c-com">Comments</label><textarea id="c-com" rows="3" bind:value={draft.comments} disabled={!jobs.canWrite}></textarea></div>
      <div class="actions">
        <Button variant="primary" onclick={save} disabled={!dirty || !nameOk || !jobs.canWrite}>Save</Button>
        <Button onclick={() => baseline && (draft = { ...baseline })} disabled={!dirty}>Revert</Button>
      </div>
      <div class="field"><span class="lbl">Played</span><span>{info.times_played} time{info.times_played === 1 ? "" : "s"} · last {when(info.last_played)}</span></div>
      <div class="field"><span class="lbl">Created</span><span>{when(info.date_created)}</span></div>
      <div class="field"><span class="lbl">Modified</span><span>{when(info.date_modified)}</span></div>
    {/if}
    <div class="actions">
      <Button onclick={() => (renaming = true)} disabled={!jobs.canWrite}>Rename…</Button>
      <Button onclick={() => (duplicating = true)} disabled={!jobs.canWrite}>Duplicate…</Button>
      <Button onclick={() => (merging = true)} disabled={!jobs.canWrite || cases.infos.length < 2} title="Merge this case with others, without repeats">Merge…</Button>
      <Button onclick={() => (deleting = true)} disabled={!jobs.canWrite}>Delete case…</Button>
    </div>
    <div class="note muted">Cases are the Pacemaker's own playlists. Order here is the order on the device.</div>
  </aside>
</div>

{#if adding}
  <CaseTrackPicker
    caseName={open.name}
    existing={new Set(cases.trackIds)}
    onadd={async (ids) => {
      const n = await cases.addTracks(open.id, ids);
      if (n !== undefined) selection.trackIds = ids;
    }}
    onclose={() => (adding = false)}
  />
{/if}

{#if menu}
  <ContextMenu x={menu.x} y={menu.y} items={menu.items} onclose={() => (menu = null)} />
{/if}

{#if duplicating}
  <NameDialog
    title="Duplicate case"
    initial={defaultCopyName}
    note={`Makes a new case with the same ${plural(all.length, "track")} in the same order and the same genre, year, rating and notes. “${open.name}” is not changed.`}
    taken={cases.infos.map((x) => x.name)}
    confirmLabel="Duplicate"
    onconfirm={async (name) => {
      duplicating = false;
      const id = await cases.duplicate(open.id, name);
      if (id !== undefined) toast.show(`Made “${name}” with ${plural(cases.infos.find((x) => x.id === id)?.track_count ?? 0, "track")}`, "ok");
    }}
    onclose={() => (duplicating = false)}
  />
{/if}

{#if merging}
  <MergeDialog initial={[open.id]} onclose={() => (merging = false)} />
{/if}

{#if transfer}
  <TransferDialog mode={transfer.mode} from={open} ids={transfer.ids} onclose={() => (transfer = null)} />
{/if}

{#if savingOrder}
  <ConfirmDialog
    title="Save this order to the case?"
    message={`Replace the order of the case “${open.name}” on the Pacemaker with the order shown (${plural(orderIds.length, "track")}). The tracks stay the same; only their order changes, and the old order is not kept.`}
    items={rows.map((r, i) => `${i + 1}. ${label(r)}`)}
    confirmLabel="Save this order"
    onconfirm={async () => {
      await cases.reorder(orderIds);
      caseView.useCaseOrder();
    }}
    onclose={() => (savingOrder = false)}
  />
{/if}

{#if renaming}
  <NameDialog
    title="Rename case"
    initial={open.name}
    note={open.name === "Favourites" ? "This is the case the heart on a track uses. If you rename it, the next heart click starts a new, empty “Favourites” case and the hearts stop showing." : undefined}
    taken={cases.infos.filter((x) => x.id !== open.id).map((x) => x.name)}
    confirmLabel="Rename"
    onconfirm={(name) => ((renaming = false), cases.rename(open.id, name))}
    onclose={() => (renaming = false)}
  />
{/if}

{#if removing}
  {@const list = removing}
  <ConfirmDialog
    title="Remove from case?"
    message={`Remove ${list.length} track${list.length === 1 ? "" : "s"} from the case “${open.name}”? ${list.length === 1 ? "It stays" : "They stay"} in the library and in any other case; only ${list.length === 1 ? "its" : "their"} place in this case is removed.`}
    items={list.map(label)}
    confirmLabel={`Remove ${list.length}`}
    onconfirm={() => {
      const gone = new Set(list.map((r) => r.id));
      selection.trackIds = selection.trackIds.filter((id) => !gone.has(id));
      cases.removeTracks([...gone]);
    }}
    onclose={() => (removing = null)}
  />
{/if}

{#if deleting}
  {@const smartUsing = cases.smartCasesUsing([open.id])}
  <ConfirmDialog
    title="Delete this case?"
    message={`Delete the case “${open.name}” (${all.length} track${all.length === 1 ? "" : "s"}) from the Pacemaker? The tracks stay in the library; only the case is removed.${open.name === "Favourites" ? " This is the case the heart on a track uses: deleting it clears every heart, and the next heart click starts a new, empty Favourites case." : ""}${smartUsing.length > 0 ? ` ${smartUsing.map((n) => `“${n}”`).join(", ")} ${smartUsing.length === 1 ? "is a smart case that has" : "are smart cases that have"} a rule about this case; ${smartUsing.length === 1 ? "it" : "they"} will report an error until you pick another case in the rule.` : ""}`}
    confirmLabel="Delete case"
    onconfirm={() => cases.remove(open.id)}
    onclose={() => (deleting = false)}
  />
{/if}

<style>
  .split {
    display: flex;
    height: 100%;
    min-height: 0;
  }
  .main {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }
  .crumb,
  .toolbar {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 10px;
    border-bottom: 1px solid var(--line);
    flex-wrap: wrap;
  }
  .hintbar {
    padding: 4px 10px;
    border-top: 1px solid var(--line);
    font-size: 12px;
  }
  .notice {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
    padding: 6px 10px;
    background: var(--accent-soft);
    border-bottom: 1px solid var(--line);
  }
  .chooser {
    position: relative;
  }
  .pop {
    position: absolute;
    z-index: 5;
    top: calc(100% + 2px);
    left: 0;
    display: grid;
    gap: 2px;
    min-width: 140px;
    padding: 6px 8px;
    background: var(--panel);
    border: 1px solid var(--line-hi);
    border-radius: 6px;
  }
  .pop label {
    display: flex;
    align-items: center;
    gap: 6px;
    cursor: pointer;
  }
  .pop input {
    padding: 0;
  }
  .warn {
    padding: 6px 10px;
    background: var(--bad-bg);
    color: var(--bad-text);
    border-bottom: 1px solid var(--line);
  }
  .scroll.dropping {
    box-shadow: inset 0 0 0 2px var(--accent);
  }
  .scroll {
    flex: 1;
    overflow: auto;
    outline: none;
  }
  table {
    width: 100%;
    border-collapse: collapse;
  }
  th.sortable {
    cursor: pointer;
  }
  th.sortable:hover {
    color: var(--text);
  }
  th.num {
    text-align: right;
  }
  td.wave {
    width: 110px;
    min-width: 100px;
  }
  .key {
    padding: 0 6px;
    border-radius: 3px;
    font-size: 12px;
    font-variant-numeric: tabular-nums;
  }
  th {
    position: sticky;
    top: 0;
    z-index: 1;
    background: var(--deep);
    text-align: left;
    font-weight: 600;
    color: var(--muted);
    padding: 6px 10px;
    border-bottom: 1px solid var(--line);
    white-space: nowrap;
  }
  td {
    padding: 4px 10px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    max-width: 260px;
  }
  .num {
    text-align: right;
    font-variant-numeric: tabular-nums;
  }
  .grip {
    width: 48px;
    user-select: none;
  }
  .heart {
    width: 26px;
    padding: 0;
    text-align: center;
  }
  tbody tr:nth-child(odd) {
    background: var(--row);
  }
  tbody tr:nth-child(even) {
    background: var(--row-alt);
  }
  tbody tr:hover {
    background: var(--hover);
  }
  tbody tr.sel {
    background: var(--accent-soft);
  }
  tbody tr:focus-visible {
    outline: 1px solid var(--accent);
    outline-offset: -1px;
  }
  tr.dim {
    color: var(--muted);
  }
  tr.moving {
    opacity: 0.45;
  }
  tr.before {
    box-shadow: inset 0 2px 0 var(--accent);
  }
  tr.after {
    box-shadow: inset 0 -2px 0 var(--accent);
  }
  td.grip[data-no-drag] {
    cursor: grab;
    touch-action: none;
  }
  .fav {
    background: none;
    border: none;
    padding: 0 4px;
    font: inherit;
    color: var(--line-hi);
    cursor: pointer;
  }
  .fav.on {
    color: var(--accent);
  }
  .fav:hover:not(:disabled) {
    color: var(--accent);
  }
  .badge {
    margin-left: 6px;
    padding: 0 6px;
    border-radius: 8px;
    font-size: 11px;
    background: var(--accent-soft);
    color: var(--accent);
  }
  .insp {
    width: 260px;
    flex: none;
    box-sizing: border-box;
    padding: 12px;
    border-left: 1px solid var(--line);
    background: var(--panel);
    overflow: auto;
  }
  .labelrow {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 4px;
    margin: 4px 0 6px;
    font-size: 12px;
  }
  .labelrow :global(.things) {
    margin-left: 0;
  }
  .insp h3 {
    margin: 0 0 2px;
    font-size: 14px;
  }
  .field {
    display: grid;
    gap: 2px;
    margin-top: 8px;
  }
  .field label,
  .lbl {
    font-size: 11px;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--muted);
  }
  .field input,
  .field textarea {
    width: 100%;
    box-sizing: border-box;
  }
  textarea {
    background: var(--panel);
    color: var(--text);
    border: 1px solid var(--line);
    border-radius: 4px;
    padding: 5px 10px;
    font: inherit;
    resize: vertical;
  }
  .actions {
    display: flex;
    gap: 6px;
    margin-top: 10px;
    flex-wrap: wrap;
  }
  .note {
    margin-top: 14px;
    font-size: 12px;
  }
</style>
