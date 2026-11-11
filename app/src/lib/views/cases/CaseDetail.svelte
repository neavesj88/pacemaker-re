<script lang="ts">
  import { untrack } from "svelte";
  import type { CaseMeta } from "$lib/api/cases";
  import type { Track } from "$lib/api/types";
  import { mmss } from "$lib/format";
  import { cases } from "$lib/state/cases.svelte";
  import { filters } from "$lib/state/filters.svelte";
  import { jobs } from "$lib/state/jobs.svelte";
  import { metadataEditor } from "$lib/state/metadata.svelte";
  import { nav } from "$lib/state/nav.svelte";
  import { playback } from "$lib/state/playback.svelte";
  import { selection } from "$lib/state/selection.svelte";
  import { session } from "$lib/state/session.svelte";
  import Button from "$lib/ui/Button.svelte";
  import Stars from "$lib/ui/Stars.svelte";
  import CaseTrackPicker from "./CaseTrackPicker.svelte";
  import ConfirmDialog from "./ConfirmDialog.svelte";
  import NameDialog from "./NameDialog.svelte";

  // The view only mounts this with a case open; the fallback covers the moment it closes.
  let open = $derived(cases.open ?? { id: -1, name: "", play_time_secs: 0, track_count: 0 });
  let info = $derived(cases.openInfo);

  // ---- rows -------------------------------------------------------------------------------
  type Row = { id: number; t: Track | undefined };
  let byId = $derived(new Map((session.library?.tracks ?? []).map((t) => [t.track_id, t])));
  // a track can sit twice in casetracks (the table has no unique constraint); show it once
  let all = $derived<Row[]>([...new Set(cases.trackIds)].map((id) => ({ id, t: byId.get(id) })));
  let missing = $derived(all.filter((r) => !r.t));
  let query = $derived(filters.query.trim().toLowerCase());
  let rows = $derived(
    query ? all.filter((r) => r.t && [r.t.title, r.t.artist, r.t.album, r.t.genre, r.t.key].some((f) => f.toLowerCase().includes(query))) : all,
  );
  let totalSecs = $derived(all.reduce((s, r) => s + (r.t?.play_time_secs ?? 0), 0));
  let selected = $derived(new Set(selection.trackIds));
  let selectedRows = $derived(all.filter((r) => selected.has(r.id)));
  // dragging needs the whole list on screen and every row to be a real track
  let canReorder = $derived(jobs.canWrite && !query && missing.length === 0);

  const label = (r: Row) => (r.t ? `${r.t.title}${r.t.artist ? ` by ${r.t.artist}` : ""}` : `track #${r.id} (no longer in the library)`);

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

  // Tracks dragged in from the Tracks view carry their ids under this type (JSON number array).
  const TRACKS_MIME = "application/x-pacemaker-tracks";
  let dropping = $state(false);

  function externalOver(e: DragEvent) {
    if (drag || !jobs.canWrite || !e.dataTransfer?.types.includes(TRACKS_MIME)) return;
    e.preventDefault();
    e.dataTransfer.dropEffect = "copy";
    dropping = true;
  }

  async function externalDrop(e: DragEvent) {
    dropping = false;
    const raw = drag ? "" : e.dataTransfer?.getData(TRACKS_MIME);
    if (!raw) return;
    e.preventDefault();
    let ids: number[] = [];
    try {
      const v = JSON.parse(raw);
      if (Array.isArray(v)) ids = v.filter((x): x is number => Number.isInteger(x));
    } catch {
      return;
    }
    if (ids.length > 0) await cases.addTracks(open.id, ids);
  }

  function dragStart(e: DragEvent, i: number) {
    const id = rows[i].id;
    const ids = selected.has(id) ? new Set(selection.trackIds) : new Set([id]);
    if (!selected.has(id)) selection.trackIds = [id];
    drag = { ids, over: i, after: false };
    e.dataTransfer?.setData("text/plain", [...ids].join(","));
    if (e.dataTransfer) e.dataTransfer.effectAllowed = "move";
  }

  function dragOver(e: DragEvent, i: number) {
    if (!drag) return;
    e.preventDefault();
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    drag = { ...drag, over: i, after: e.clientY > r.top + r.height / 2 };
  }

  function drop(e: DragEvent) {
    e.preventDefault();
    if (!drag) return;
    moveBlock(drag.ids, drag.over + (drag.after ? 1 : 0));
    drag = null;
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

  function play(id?: number) {
    const target = id ?? selectedRows.find((r) => r.t)?.id ?? all.find((r) => r.t)?.id;
    if (target !== undefined) playback.load(target, true);
  }

  function onkey(e: KeyboardEvent) {
    // Not while a dialog is up: its keys must not act on the case behind it.
    if (adding || renaming || deleting || removing) return;
    if ((e.target as HTMLElement).closest("input, select, textarea, [role='dialog']")) return;
    if (e.key === "Delete" && jobs.canWrite && selectedRows.length > 0) removing = selectedRows;
    else if (e.altKey && e.key === "ArrowUp") (e.preventDefault(), nudge(-1));
    else if (e.altKey && e.key === "ArrowDown") (e.preventDefault(), nudge(1));
  }

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

<svelte:window onkeydown={onkey} />

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
        onclick={() => metadataEditor.edit(selectedRows.filter((r) => r.t).map((r) => r.id))}
        disabled={selectedRows.every((r) => !r.t)}
      >Edit tracks…</Button>
      <span class="muted hint">Drag rows to reorder, or drop tracks here from the Tracks view · the order is the order on the Pacemaker</span>
    </div>

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
      ondragover={externalOver}
      ondragleave={() => (dropping = false)}
      ondrop={externalDrop}
    >
      <table role="grid" aria-multiselectable="true" aria-label="Tracks in the case">
        <thead>
          <tr>
            <th class="grip">#</th>
            <th class="heart"></th>
            <th>Title</th>
            <th>Artist</th>
            <th class="num">BPM</th>
            <th>Key</th>
            <th>Rating</th>
            <th class="num">Time</th>
          </tr>
        </thead>
        <tbody bind:this={tbody}>
          {#each rows as r, i (r.id)}
            <tr
              tabindex={i === Math.min(focusIdx, rows.length - 1) ? 0 : -1}
              aria-selected={selected.has(r.id)}
              onkeydown={(e) => rowKey(e, i)}
              draggable={canReorder}
              class:sel={selected.has(r.id)}
              class:dim={!r.t || session.needs.has(r.id)}
              class:before={drag && drag.over === i && !drag.after && !drag.ids.has(r.id)}
              class:after={drag && drag.over === i && drag.after && !drag.ids.has(r.id)}
              class:moving={drag?.ids.has(r.id)}
              onclick={(e) => pick(i, e)}
              ondblclick={() => r.t && play(r.id)}
              ondragstart={(e) => dragStart(e, i)}
              ondragover={(e) => dragOver(e, i)}
              ondrop={drop}
              ondragend={() => (drag = null)}
            >
              <td class="grip muted">{canReorder ? "≡ " : ""}{all.indexOf(r) + 1}</td>
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
              {#if r.t}
                <td>{r.t.title}{#if session.needs.has(r.id)}<span class="badge" title={session.needs.get(r.id)}>needs analysis</span>{/if}</td>
                <td>{r.t.artist}</td>
                <td class="num">{r.t.bpm && !session.needs.has(r.id) ? r.t.bpm.toFixed(2) : "–"}</td>
                <td>{r.t.key}</td>
                <td><Stars rating={r.t.rating} /></td>
                <td class="num">{mmss(r.t.play_time_secs)}</td>
              {:else}
                <td colspan="6">Track #{r.id} is no longer in the library</td>
              {/if}
            </tr>
          {/each}
        </tbody>
      </table>
      {#if rows.length === 0}
        <p class="empty">
          {query ? "No tracks in this case match the search." : "This case is empty. Use Add tracks…, or select tracks in the Tracks view and make a case from them."}
        </p>
      {/if}
    </div>
  </div>

  <aside class="insp">
    <h3>{info?.name ?? open.name}</h3>
    <div class="muted">case · {all.length} track{all.length === 1 ? "" : "s"} · {mmss(totalSecs)}</div>
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
  <ConfirmDialog
    title="Delete this case?"
    message={`Delete the case “${open.name}” (${all.length} track${all.length === 1 ? "" : "s"}) from the Pacemaker? The tracks stay in the library; only the case is removed.${open.name === "Favourites" ? " This is the case the heart on a track uses: deleting it clears every heart, and the next heart click starts a new, empty Favourites case." : ""}`}
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
  .hint {
    margin-left: auto;
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
  tr[draggable="true"] {
    cursor: grab;
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
