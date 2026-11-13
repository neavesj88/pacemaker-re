<script lang="ts">
  import { cases } from "$lib/state/cases.svelte";
  import { jobs } from "$lib/state/jobs.svelte";
  import { nav } from "$lib/state/nav.svelte";
  import { selection } from "$lib/state/selection.svelte";
  import { insertIndex } from "$lib/state/dragLogic";
  import { armReorder } from "$lib/state/pointerReorder.svelte";
  import { session } from "$lib/state/session.svelte";
  import Button from "$lib/ui/Button.svelte";
  import ConfirmDialog from "$lib/views/cases/ConfirmDialog.svelte";
  import { keyColour, parseKey } from "$lib/views/tracks/trackLogic";
  import CreateMixDialog from "./CreateMixDialog.svelte";
  import SaveCaseDialog from "./SaveCaseDialog.svelte";
  import SetSettings from "./SetSettings.svelte";
  import { setPrefs } from "./prefs.svelte";
  import { setBuilder } from "./setbuilder.svelte";
  import { clock, lengthLine, playSecs, sectionFitText, solverOptions, type Transition } from "./solver";

  // The set builder: pick a case (or the selected tracks), order them into a set, adjust it by
  // hand, then save it as a case or lay it out as a mix. Nothing here changes the device until
  // one of those two buttons is confirmed.

  let o = $derived(solverOptions(setPrefs.value));
  let sum = $derived(setBuilder.summary);
  let hasEnergy = $derived([...setBuilder.byId.values()].some((t) => t.energy != null));

  // ---- source ------------------------------------------------------------------------------
  let choice = $derived(setBuilder.source?.kind === "case" ? `case:${setBuilder.source.id}` : setBuilder.source ? "selection" : "");

  function pickSource(v: string) {
    if (v === "selection") void setBuilder.loadSelection();
    else if (v.startsWith("case:")) {
      const c = session.library?.cases.find((x) => x.id === Number(v.slice(5)));
      if (c) void setBuilder.load({ kind: "case", id: c.id, name: c.name });
    }
  }

  // Loading replaces the set, so work done by hand (locks, left-out tracks, edits) is named and
  // confirmed first. `want` is a picker value, or "reload" for the current source.
  let confirmLoad = $state<string | null>(null);

  function requestLoad(want: string, select?: HTMLSelectElement) {
    if (!setBuilder.hasHandWork) return runLoad(want);
    // the picker keeps showing the current source until the load is confirmed
    if (select) select.value = choice;
    confirmLoad = want;
  }

  function runLoad(want: string) {
    if (want === "reload") void setBuilder.reload();
    else pickSource(want);
  }

  // Deep links: `nav.go("setbuilder", { "setbuilder.caseId": 3 })` or `{ "setbuilder.trackIds": [..] }`.
  $effect(() => {
    const caseId = nav.params["setbuilder.caseId"];
    const ids = nav.params["setbuilder.trackIds"];
    if (typeof caseId === "number") {
      nav.take("setbuilder.caseId");
      const c = session.library?.cases.find((x) => x.id === caseId);
      if (c) void setBuilder.load({ kind: "case", id: c.id, name: c.name });
    } else if (Array.isArray(ids)) {
      nav.take("setbuilder.trackIds");
      void setBuilder.load({ kind: "tracks", name: "Selected tracks", ids: ids.filter((x): x is number => typeof x === "number") });
    }
  });

  // ---- rows --------------------------------------------------------------------------------
  let sel = $state<number[]>([]);
  let anchor = $state<number | null>(null);
  let focusIdx = $state(0);
  let tbody = $state<HTMLElement>();
  let rows = $derived(setBuilder.order.filter((id) => setBuilder.byId.has(id)));
  let selSet = $derived(new Set(sel));

  const rowOf = (id: number) => setBuilder.rows.get(id);
  const titleOf = (id: number) => rowOf(id)?.title || `track #${id}`;
  const labelOf = (id: number) => {
    const t = rowOf(id);
    return t ? `${t.title}${t.artist ? ` by ${t.artist}` : ""}` : `track #${id}`;
  };

  function pick(i: number, e: MouseEvent) {
    focusIdx = i;
    const id = rows[i];
    if (e.shiftKey && anchor !== null && anchor < rows.length) {
      const [a, b] = [Math.min(anchor, i), Math.max(anchor, i)];
      sel = rows.slice(a, b + 1);
    } else if (e.ctrlKey || e.metaKey) {
      sel = selSet.has(id) ? sel.filter((x) => x !== id) : [...sel, id];
      anchor = i;
    } else {
      sel = [id];
      anchor = i;
    }
  }

  function rowKey(e: KeyboardEvent, i: number) {
    if (e.ctrlKey || e.metaKey || (e.target as HTMLElement).closest("button")) return;
    const id = rows[i];
    if (e.altKey && (e.key === "ArrowUp" || e.key === "ArrowDown")) {
      e.preventDefault();
      setBuilder.nudge(sel.length ? sel : [id], e.key === "ArrowUp" ? -1 : 1);
    } else if (e.altKey) {
      return;
    } else if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      const n = Math.max(0, Math.min(rows.length - 1, i + (e.key === "ArrowDown" ? 1 : -1)));
      sel = [rows[n]];
      anchor = n;
      focusIdx = n;
      (tbody?.querySelectorAll<HTMLElement>("tr.track")[n])?.focus();
    } else if (e.key === "l" || e.key === "L") {
      e.preventDefault();
      for (const x of sel.length ? sel : [id]) setBuilder.toggleLock(x);
    } else if (e.key === "Delete") {
      e.preventDefault();
      setBuilder.leaveOut(sel.length ? sel : [id]);
      sel = [];
    }
  }

  // ---- drag to reorder ---------------------------------------------------------------------
  let drag = $state<{ ids: Set<number>; over: number; after: boolean } | null>(null);

  // Pointer events, not HTML5 drag and drop (see state/pointerReorder.svelte.ts): the Windows app
  // does not get in-page dragover / drop events.
  function armDrag(e: PointerEvent, i: number) {
    const id = rows[i];
    armReorder(e, {
      rows: () => [...(tbody?.querySelectorAll<HTMLElement>("tr.track") ?? [])],
      scroller: () => tbody?.closest<HTMLElement>(".scroll"),
      begin: () => {
        const ids = selSet.has(id) ? new Set(sel) : new Set([id]);
        if (!selSet.has(id)) sel = [id];
        drag = { ids, over: i, after: false };
      },
      over: (p) => {
        if (drag) drag = { ...drag, ...p };
      },
      drop: (p) => {
        if (drag) setBuilder.move([...drag.ids], insertIndex(p));
      },
      end: () => (drag = null),
    });
  }

  // ---- strip words -------------------------------------------------------------------------
  const scoreClass = (s: number) => (s >= 75 ? "good" : s >= 50 ? "mid" : "poor");

  function tempoText(t: Transition): string {
    if (t.relation === "unknown") return "tempo unknown";
    const d = `${t.deltaPct >= 0 ? "+" : "−"}${Math.abs(t.deltaPct).toFixed(1)} %`;
    const rel = t.relation === "double" ? ", double time" : t.relation === "half" ? ", half time" : "";
    return `${t.bpmFrom.toFixed(1)} → ${t.bpmTo.toFixed(1)} BPM (${d}${rel})`;
  }

  const capped = (id: number) => {
    const t = setBuilder.byId.get(id);
    return !!t && o.maxTrackSecs != null && t.secs > o.maxTrackSecs;
  };

  // ---- actions -----------------------------------------------------------------------------
  let savingCase = $state(false);
  let creatingMix = $state(false);
  let canAct = $derived(jobs.canWrite && rows.length > 0);

  function onkey(e: KeyboardEvent) {
    if (savingCase || creatingMix) return;
    if ((e.target as HTMLElement).closest("input, select, textarea, [role='dialog']")) return;
    if (e.key === "z" && (e.ctrlKey || e.metaKey) && setBuilder.canUndo) {
      e.preventDefault();
      setBuilder.undo();
    }
  }

  let selectionCount = $derived(selection.trackIds.length);
</script>

<svelte:window onkeydown={onkey} />

<div class="split">
  <div class="main">
    <div class="bar">
      <label class="src">
        <span class="lbl">Tracks from</span>
        <select value={choice} onchange={(e) => requestLoad(e.currentTarget.value, e.currentTarget)} aria-label="Where the tracks come from">
          <option value="" disabled>Choose a case or the selected tracks…</option>
          <option value="selection" disabled={selectionCount === 0}>Selected tracks ({selectionCount})</option>
          {#if (session.library?.cases.length ?? 0) > 0}
            <optgroup label="Cases">
              {#each session.library?.cases ?? [] as c (c.id)}
                <option value={`case:${c.id}`}>{c.name} ({c.track_count})</option>
              {/each}
            </optgroup>
          {/if}
        </select>
      </label>
      {#if setBuilder.source}
        <Button onclick={() => requestLoad("reload")} title="Read the source again. Locks, tracks you left out and your edits are dropped (you are asked first)" disabled={setBuilder.loading}>Reload</Button>
      {/if}
      <span class="sp"></span>
      <Button variant="primary" onclick={() => setBuilder.solve()} disabled={rows.length + setBuilder.out.length < 2}>
        {rows.length > 0 ? "Re-solve" : "Order the set"}
      </Button>
      <Button onclick={() => setBuilder.undo()} disabled={!setBuilder.canUndo} title={setBuilder.canUndo ? `Undo: ${setBuilder.undoLabel} (Ctrl+Z)` : "Nothing to undo"}>Undo</Button>
      <Button onclick={() => setBuilder.restoreSourceOrder()} disabled={!setBuilder.source} title="Back to the order the tracks came in; left-out tracks come back and locks are cleared">Original order</Button>
      <Button onclick={() => (savingCase = true)} disabled={!canAct}>Save as case…</Button>
      <Button onclick={() => (creatingMix = true)} disabled={!canAct}>Create mix…</Button>
    </div>

    {#if !setBuilder.source}
      <div class="empty">
        <p>Pick a case, or select tracks in the Tracks view, to build a set from.</p>
        <p class="muted">The builder orders them by key and tempo, lets you lock, reorder and leave tracks out, then saves the result as a new case or lays it out as a mix with the beat-match and transition tools. It never changes the case you start from.</p>
      </div>
    {:else}
      <div class="sum" role="status">
        <b>{setBuilder.source.name}</b>
        <span>{rows.length} track{rows.length === 1 ? "" : "s"}</span>
        <span title="Sum of the play times (capped per track). A mix made from it is shorter by the transition overlaps.">{lengthLine(sum.secs, o, setBuilder.short)}</span>
        {#if sum.avgScore !== null}
          <span>average transition <b class={scoreClass(sum.avgScore)}>{Math.round(sum.avgScore)}</b></span>
          {#if sum.minScore !== null && sum.minAt !== null}
            <span>weakest <b class={scoreClass(sum.minScore)}>{sum.minScore}</b> (after {titleOf(rows[sum.minAt])})</span>
          {/if}
          <span class:bad={sum.violations > 0}>{sum.violations === 0 ? "no limit broken" : `${sum.violations} transition${sum.violations === 1 ? "" : "s"} break a limit`}</span>
        {/if}
        {#if setBuilder.edited}<span class="tag" title="You changed the set or the settings since it was last solved">changed since solve</span>{/if}
        {#if setBuilder.pinned.size > 0}<span class="muted">{setBuilder.pinned.size} locked</span>{/if}
      </div>

      {#if setBuilder.notice}<div class="warn" role="status">{setBuilder.notice}</div>{/if}

      {#if setBuilder.gone.length > 0 || setBuilder.missing.length > 0}
        <div class="warn">
          {#if setBuilder.gone.length > 0}
            {setBuilder.gone.length} entr{setBuilder.gone.length === 1 ? "y" : "ies"} of the source {setBuilder.gone.length === 1 ? "is" : "are"} no longer in the library and {setBuilder.gone.length === 1 ? "is" : "are"} not in the set.
          {/if}
          {#if setBuilder.missing.length > 0}
            {setBuilder.missing.length} track{setBuilder.missing.length === 1 ? " was" : "s were"} removed from the library since this set was made.
            <Button small onclick={() => setBuilder.dropMissing()}>Drop {setBuilder.missing.length === 1 ? "it" : "them"}</Button>
          {/if}
        </div>
      {/if}

      <div class="hintbar muted">
        <span>Drag rows to reorder (Alt+Up / Down moves the selection). L locks a track where it is; Delete leaves it out. A locked track keeps its position when the set is solved again.</span>
        {#if rows.length > 0}
          <Button small variant="ghost" onclick={() => setBuilder.lockAll(setBuilder.pinned.size < rows.length)}>{setBuilder.pinned.size < rows.length ? "Lock all" : "Unlock all"}</Button>
          <Button small variant="ghost" onclick={() => setBuilder.leaveOut(sel)} disabled={sel.length === 0}>Leave out{sel.length ? ` ${sel.length}` : ""}</Button>
        {/if}
      </div>

      <div class="scroll" role="region" aria-label="The set" tabindex="-1">
        <table aria-label="Tracks of the set in play order">
          <thead>
            <tr>
              <th class="grip">#</th>
              <th class="lock" aria-label="Locked"></th>
              <th>Title</th>
              <th class="num">BPM</th>
              <th>Key</th>
              <th class="num">Energy</th>
              <th class="num">Plays for</th>
            </tr>
          </thead>
          <tbody bind:this={tbody}>
            {#each rows as id, i (id)}
              {@const st = setBuilder.byId.get(id)!}
              {@const key = parseKey(rowOf(id)?.key ?? "")}
              {@const locked = setBuilder.pinned.has(id)}
              <tr
                class="track"
                tabindex={i === Math.min(focusIdx, rows.length - 1) ? 0 : -1}
                aria-selected={selSet.has(id)}
                class:sel={selSet.has(id)}
                class:before={drag && drag.over === i && !drag.after && !drag.ids.has(id)}
                class:after={drag && drag.over === i && drag.after && !drag.ids.has(id)}
                class:moving={drag?.ids.has(id)}
                onclick={(e) => pick(i, e)}
                onkeydown={(e) => rowKey(e, i)}
                onpointerdown={(e) => armDrag(e, i)}
              >
                <td class="grip muted">≡ {i + 1}</td>
                <td class="lock">
                  <button
                    class="lockbtn"
                    class:on={locked}
                    aria-pressed={locked}
                    aria-label={locked ? `Unlock ${titleOf(id)} (locked at position ${i + 1})` : `Lock ${titleOf(id)} at position ${i + 1}`}
                    title={locked ? `Locked at position ${i + 1}. Click to unlock.` : "Lock at this position"}
                    onclick={(e) => (e.stopPropagation(), setBuilder.toggleLock(id))}
                  >{locked ? "locked" : "lock"}</button>
                </td>
                <td class="title">
                  <span>{titleOf(id)}</span>
                  {#if rowOf(id)?.artist}<span class="muted"> · {rowOf(id)?.artist}</span>{/if}
                  {#if session.needs.has(id)}<span class="badge" title={session.needs.get(id)}>needs analysis</span>{/if}
                </td>
                <td class="num">{st.bpm > 0 ? st.bpm.toFixed(2) : "–"}</td>
                <td>
                  {#if key}<span class="key" style:--k={keyColour(key)}>{key.code}</span>{:else}<span class="muted" title={rowOf(id)?.key ? `“${rowOf(id)?.key}” is not a key the builder reads` : "no key"}>–</span>{/if}
                </td>
                <td class="num">{st.energy ?? "–"}</td>
                <td class="num" title={capped(id) ? `The track is ${clock(st.secs)} long; it plays for at most ${clock(playSecs(st, o))}` : ""}>
                  {#if capped(id)}<span class="muted">{clock(st.secs)} →</span> {/if}{clock(playSecs(st, o))}
                </td>
              </tr>
              {#if i < rows.length - 1 && sum.transitions[i]}
                {@const t = sum.transitions[i]}
                <tr class="strip" aria-label={`Transition from ${titleOf(id)} to ${titleOf(rows[i + 1])}: score ${t.score}`}>
                  <td colspan="7">
                    <div class="trans {scoreClass(t.score)}">
                      <span class="bar2" aria-hidden="true"><i style:width="{t.score}%"></i></span>
                      <b class="score" title="0 to 100">{t.score}</b>
                      <span class:bad={t.keyOk === false}>{t.fromKey} → {t.toKey} · {t.moveLabel}</span>
                      <span class:bad={t.bpmOk === false}>{tempoText(t)}</span>
                      {#if t.energyDelta !== null}<span class="muted">energy {t.energyDelta >= 0 ? "+" : "−"}{Math.abs(t.energyDelta)}</span>{/if}
                      {#if sectionFitText(t)}<span class="muted" title="From the tracks' detected sections (estimates)">{sectionFitText(t)}</span>{/if}
                    </div>
                  </td>
                </tr>
              {/if}
            {/each}
          </tbody>
        </table>
        {#if rows.length === 0}
          <p class="empty">{setBuilder.loading ? "Reading the tracks…" : "There are no tracks in the set."}</p>
        {/if}

        {#if setBuilder.out.length > 0}
          <h4 class="outh">Left out ({setBuilder.out.length})</h4>
          <table aria-label="Tracks left out of the set" class="outt">
            <tbody>
              {#each setBuilder.out as x (x.id)}
                <tr>
                  <td class="title">{labelOf(x.id)}</td>
                  <td class="muted">{x.why === "you" ? "left out by you" : "dropped to get closer to the target length"}</td>
                  <td class="num">{clock(setBuilder.byId.get(x.id) ? playSecs(setBuilder.byId.get(x.id)!, o) : 0)}</td>
                  <td class="act"><Button small onclick={() => setBuilder.addBack(x.id)}>Add back</Button></td>
                </tr>
              {/each}
            </tbody>
          </table>
        {/if}
      </div>
    {/if}
  </div>

  <SetSettings {hasEnergy} />
</div>

{#if confirmLoad !== null}
  <ConfirmDialog
    title={confirmLoad === "reload" ? "Read the source again?" : "Load other tracks?"}
    message={confirmLoad === "reload" ? "The set is read from its source again and ordered afresh. This would be lost:" : "The set is replaced by the tracks you picked. This would be lost:"}
    items={setBuilder.lostByLoading()}
    confirmLabel={confirmLoad === "reload" ? "Read again" : "Replace the set"}
    onconfirm={() => runLoad(confirmLoad!)}
    onclose={() => (confirmLoad = null)}
  />
{/if}
{#if savingCase}
  <SaveCaseDialog onclose={() => (savingCase = false)} />
{/if}
{#if creatingMix}
  <CreateMixDialog onclose={() => (creatingMix = false)} />
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
  .bar,
  .sum,
  .hintbar {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 10px;
    border-bottom: 1px solid var(--line);
    flex-wrap: wrap;
  }
  .sum {
    gap: 14px;
    background: var(--deep);
  }
  .hintbar {
    font-size: 12px;
    padding: 4px 10px;
  }
  .hintbar span:first-child {
    flex: 1;
  }
  .src {
    display: flex;
    gap: 6px;
    align-items: center;
  }
  .lbl {
    font-size: 11px;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--muted);
  }
  .sp {
    flex: 1;
  }
  .warn {
    padding: 6px 10px;
    background: var(--bad-bg);
    color: var(--bad-text);
    border-bottom: 1px solid var(--line);
  }
  .empty {
    padding: 24px;
    max-width: 560px;
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
  }
  td.title {
    overflow: hidden;
    text-overflow: ellipsis;
    max-width: 360px;
  }
  .num {
    text-align: right;
    font-variant-numeric: tabular-nums;
  }
  .grip {
    width: 56px;
    user-select: none;
  }
  .lock {
    width: 56px;
    padding: 0 4px;
  }
  tr.track {
    background: var(--row);
    cursor: grab;
  }
  tr.track:hover {
    background: var(--hover);
  }
  tr.track.sel {
    background: var(--accent-soft);
  }
  tr.track:focus-visible {
    outline: 1px solid var(--accent);
    outline-offset: -1px;
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
  tr.strip td {
    padding: 0 10px 0 56px;
    background: var(--bg);
  }
  .trans {
    display: flex;
    align-items: center;
    gap: 12px;
    font-size: 12px;
    padding: 2px 0;
    color: var(--muted);
  }
  .bar2 {
    width: 60px;
    height: 6px;
    background: var(--line);
    border-radius: 3px;
    overflow: hidden;
    flex: none;
  }
  .bar2 i {
    display: block;
    height: 100%;
    background: var(--muted);
  }
  .good .bar2 i {
    background: var(--ok);
  }
  .mid .bar2 i {
    background: var(--cue);
  }
  .poor .bar2 i {
    background: var(--bad);
  }
  .score {
    width: 24px;
    text-align: right;
    font-variant-numeric: tabular-nums;
  }
  .trans.good .score,
  b.good {
    color: var(--ok);
  }
  .trans.mid .score,
  b.mid {
    color: var(--cue);
  }
  .trans.poor .score,
  b.poor {
    color: var(--bad);
  }
  .bad {
    color: var(--bad-text);
  }
  .lockbtn {
    background: none;
    border: 1px solid transparent;
    border-radius: 4px;
    padding: 1px 6px;
    font: inherit;
    font-size: 11px;
    color: var(--line-hi);
    cursor: pointer;
  }
  .lockbtn:hover {
    border-color: var(--line-hi);
    color: var(--text);
  }
  .lockbtn.on {
    color: var(--accent);
    border-color: var(--accent);
  }
  .key {
    display: inline-block;
    min-width: 28px;
    text-align: center;
    padding: 0 6px;
    border-radius: 8px;
    font-size: 12px;
    color: var(--bg);
    background: var(--k);
  }
  .badge,
  .tag {
    margin-left: 6px;
    padding: 0 6px;
    border-radius: 8px;
    font-size: 11px;
    background: var(--accent-soft);
    color: var(--accent);
  }
  .tag {
    margin-left: 0;
  }
  .outh {
    margin: 16px 10px 4px;
    font-size: 12px;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--muted);
  }
  .outt td.act {
    text-align: right;
  }
</style>
