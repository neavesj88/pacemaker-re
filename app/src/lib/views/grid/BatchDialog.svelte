<script lang="ts">
  // The batch beat grid dialog: pick an operation for the chosen tracks, preview what it does to
  // every one of them (old -> new BPM and first beat), look at the old and the new grid of any
  // track, untick the ones to leave, see which mixes would drift, then confirm the write.
  import { changing, counts, firstShiftMs, fmtBpm, fmtFrames, fmtShift, KIND_LABEL, mixesWarning, OP_LABEL, type BatchKind } from "$lib/state/batchLogic";
  import { gridBatch } from "$lib/state/gridBatch.svelte";
  import { suspects } from "$lib/state/gridSuspects.svelte";
  import { jobs } from "$lib/state/jobs.svelte";
  import { session } from "$lib/state/session.svelte";
  import Button from "$lib/ui/Button.svelte";
  import { focusFirst } from "$lib/ui/focus";
  import GridDiff from "./GridDiff.svelte";

  const KINDS: { kind: BatchKind; hint: string }[] = [
    { kind: "nudge", hint: "Moves every beat line by the same time. Tempo and bar lines stay." },
    { kind: "halve", hint: "Half the tempo: every second beat line is kept (the grid editor's ½ BPM)." },
    { kind: "double", hint: "Double the tempo: a beat line is added midway between every two (the grid editor's ×2 BPM)." },
    {
      kind: "suggested",
      hint: "Halve or double each track the tempo rule flagged, whichever brings it into its genre's window. Tracks the rule does not flag are left alone.",
    },
    { kind: "reanalyse", hint: "Runs the analyser on the audio again. Nothing is saved until you have looked at the result." },
  ];

  const lib = $derived(session.library);
  const hint = $derived(KINDS.find((k) => k.kind === gridBatch.kind)?.hint ?? "");
  const rows = $derived(gridBatch.rows);
  const tally = $derived(counts(rows, gridBatch.ticked));
  const focus = $derived(rows.find((r) => r.track_id === gridBatch.focusId) ?? null);
  const warning = $derived(mixesWarning(gridBatch.mixes));
  const everyChanging = $derived(changing(rows).length);
  const allTicked = $derived(everyChanging > 0 && tally.change === everyChanging);
  const rulesProblem = $derived(gridBatch.kind === "suggested" ? suspects.error : "");
  const progressPct = $derived(gridBatch.progress && gridBatch.progress.total > 0 ? Math.round((100 * gridBatch.progress.done) / gridBatch.progress.total) : 0);

  const n = (count: number, word: string) => `${count} ${word}${count === 1 ? "" : "s"}`;
  const mixTitle = (names: string[]) => names.join(", ");
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && gridBatch.phase !== "working" && gridBatch.close()} />

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="scrim" onclick={() => gridBatch.close()}>
  <div class="dlg" role="dialog" aria-modal="true" aria-label="Batch beat grid tools" tabindex="-1" use:focusFirst onclick={(e) => e.stopPropagation()}>
    <header>
      <h2>Batch beat grid tools</h2>
      <span class="muted">{n(gridBatch.ids.length, "track")} on {lib?.root ?? ""}</span>
    </header>

    <section class="controls" aria-label="Operation">
      <div class="kinds" role="radiogroup" aria-label="What to do">
        {#each KINDS as k (k.kind)}
          <Button
            small
            active={gridBatch.kind === k.kind}
            role="radio"
            aria-checked={gridBatch.kind === k.kind}
            disabled={gridBatch.phase === "working"}
            onclick={() => (gridBatch.kind = k.kind)}
          >{KIND_LABEL[k.kind]}</Button>
        {/each}
        {#if gridBatch.kind === "nudge"}
          <label class="nudge">
            by
            <input
              inputmode="decimal"
              value={gridBatch.nudgeText}
              oninput={(e) => gridBatch.setNudge(e.currentTarget.value)}
              onkeydown={(e) => e.key === "Enter" && gridBatch.preview()}
              disabled={gridBatch.phase === "working"}
              aria-label="Nudge in milliseconds, negative is earlier"
              aria-invalid={"error" in gridBatch.nudge}
            />
            ms
          </label>
        {/if}
        {#if gridBatch.kind === "suggested"}
          <Button small variant="ghost" onclick={() => (suspects.editing = true)} title="The genre windows and the confidence floor the suggestion follows">Rules…</Button>
        {/if}
        <span class="spacer"></span>
        {#if gridBatch.phase === "working"}
          <Button small onclick={() => gridBatch.stop()}>Stop</Button>
        {:else}
          <Button small variant="primary" onclick={() => gridBatch.preview()} disabled={gridBatch.kind === "nudge" && "error" in gridBatch.nudge}>
            {gridBatch.phase === "review" ? (gridBatch.stale ? "Preview again" : "Refresh preview") : "Preview"}
          </Button>
        {/if}
      </div>
      <p class="muted hint">{hint}{gridBatch.kind === "nudge" ? " Negative moves earlier." : ""}</p>
      {#if gridBatch.kind === "nudge" && "error" in gridBatch.nudge}<p class="err" role="alert">{gridBatch.nudge.error}</p>{/if}
      {#if rulesProblem}<p class="err" role="alert">{rulesProblem}</p>{/if}
      {#if gridBatch.error}<p class="err" role="alert">{gridBatch.error}</p>{/if}
    </section>

    {#if gridBatch.phase === "setup"}
      <p class="empty">Choose what to do, then Preview. Nothing is changed on the device until you have seen the result for every track and confirmed.</p>
    {:else if gridBatch.phase === "working"}
      <div class="working" role="status">
        <div class="bar" aria-label="Analysis progress"><i style:width="{progressPct}%"></i></div>
        <p>
          Analysing {gridBatch.progress?.done ?? 0} of {gridBatch.progress?.total ?? gridBatch.ids.length}
          {#if gridBatch.progress?.label}<span class="muted">· {gridBatch.progress.label}</span>{/if}
        </p>
        <p class="muted">Reading the audio only; nothing is written. Stop keeps the tracks analysed so far.</p>
      </div>
    {:else}
      <div class="summary" aria-live="polite">
        <strong>{n(tally.change, "track")} will change</strong>
        {#if tally.unticked > 0}<span>· {tally.unticked} unticked</span>{/if}
        {#if tally.skipped > 0}<span>· {tally.skipped} left as they are</span>{/if}
        <span class="spacer"></span>
        <Button small variant="link" onclick={() => gridBatch.tickAll(!allTicked)} disabled={everyChanging === 0}>{allTicked ? "Untick all" : "Tick all"}</Button>
      </div>
      {#if gridBatch.stale}
        <p class="stale" role="alert">The controls changed since this preview. Press Preview again before applying.</p>
      {/if}

      <div class="body" class:dim={gridBatch.stale}>
        <div class="tablewrap">
          <table>
            <thead>
              <tr>
                <th class="chk"><span class="sr">Change</span></th>
                <th>Track</th>
                <th>Change</th>
                <th class="r">BPM</th>
                <th class="r">First beat</th>
                <th class="r">Mixes</th>
              </tr>
            </thead>
            <tbody>
              {#each rows as r (r.track_id)}
                {@const ticked = gridBatch.ticked.has(r.track_id)}
                <tr class:left={r.skip !== null} class:off={r.skip === null && !ticked} class:focus={gridBatch.focusId === r.track_id} onclick={() => (gridBatch.focusId = r.track_id)}>
                  <td class="chk">
                    <input
                      type="checkbox"
                      checked={r.skip === null && ticked}
                      disabled={r.skip !== null}
                      onclick={(e) => e.stopPropagation()}
                      onchange={(e) => gridBatch.toggle(r.track_id, e.currentTarget.checked)}
                      aria-label="Change {r.label}"
                    />
                  </td>
                  <td class="name" title={r.label}>
                    <button class="rowbtn" onclick={() => (gridBatch.focusId = r.track_id)}>{r.label}</button>
                  </td>
                  {#if r.skip !== null}
                    <td colspan="4" class="why">Left as it is: {r.skip}</td>
                  {:else}
                    <td class="op">{ticked ? (OP_LABEL[r.op] ?? r.op) : "Unticked"}</td>
                    <td class="r num">{fmtBpm(r.before_bpm)} → <strong>{fmtBpm(r.after_bpm)}</strong></td>
                    <td class="r num">
                      {fmtFrames(r.before_first, r.sample_rate)} → <strong>{fmtFrames(r.after_first, r.sample_rate)}</strong>
                      <span class="muted shift">{fmtShift(firstShiftMs(r))}</span>
                    </td>
                    <td class="r num">
                      {#if r.mixes.length > 0}
                        <span class="mixchip" title={mixTitle(r.mixes.map((m) => m.name))}>{r.mixes.length}</span>
                      {:else}<span class="muted">–</span>{/if}
                    </td>
                  {/if}
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
        <aside class="side">
          {#if focus && focus.skip === null && gridBatch.plan}
            <h3>{focus.label}</h3>
            <GridDiff root={lib?.root ?? ""} planId={gridBatch.plan.plan_id} row={focus} />
          {:else if focus}
            <h3>{focus.label}</h3>
            <p class="muted">Left as it is: {focus.skip}.</p>
          {:else}
            <p class="muted">Click a track to see its old and new grid.</p>
          {/if}
        </aside>
      </div>

      {#if warning}
        <p class="mixwarn" role="note">{warning}</p>
      {/if}
    {/if}

    <footer>
      <Button onclick={() => gridBatch.close()} disabled={gridBatch.phase === "working"}>Close</Button>
      {#if gridBatch.phase === "review"}
        <Button variant="primary" disabled={tally.change === 0 || gridBatch.stale || !jobs.canWrite} onclick={() => gridBatch.apply()}>
          {tally.change === 0 ? "Nothing to apply" : `Apply to ${n(tally.change, "track")}…`}
        </Button>
      {/if}
    </footer>
  </div>
</div>

<style>
  .scrim {
    position: fixed;
    inset: 0;
    z-index: 30;
    display: flex;
    align-items: center;
    justify-content: center;
    background: var(--scrim);
  }
  .dlg {
    display: flex;
    flex-direction: column;
    gap: 10px;
    width: min(1220px, 96vw);
    height: min(88vh, 920px);
    padding: 16px 18px;
    background: var(--panel);
    border: 1px solid var(--line-hi);
    border-radius: 8px;
  }
  header {
    display: flex;
    align-items: baseline;
    gap: 12px;
  }
  h2 {
    margin: 0;
    font-size: 16px;
  }
  h3 {
    margin: 0 0 6px;
    font-size: 13px;
    overflow-wrap: anywhere;
  }
  .controls .kinds {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
  }
  .nudge {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }
  .nudge input {
    width: 72px;
  }
  .spacer {
    flex: 1;
  }
  .hint {
    margin: 6px 0 0;
  }
  .err {
    margin: 6px 0 0;
    color: var(--bad-text);
    font-size: 12px;
  }
  .empty {
    margin: 0;
    padding: 24px 0;
    color: var(--muted);
  }
  .working {
    padding: 12px 0;
  }
  .working p {
    margin: 6px 0 0;
  }
  .bar {
    height: 8px;
    border-radius: 4px;
    background: var(--deep);
    border: 1px solid var(--line);
    overflow: hidden;
  }
  .bar i {
    display: block;
    height: 100%;
    background: var(--accent-ramp);
  }
  .summary {
    display: flex;
    align-items: center;
    gap: 10px;
    flex-wrap: wrap;
  }
  .stale {
    margin: 0;
    padding: 6px 10px;
    background: var(--bad-bg);
    color: var(--bad-text);
    border-radius: 4px;
  }
  .body {
    display: grid;
    grid-template-columns: minmax(0, 1.45fr) minmax(0, 1fr);
    gap: 14px;
    flex: 1;
    min-height: 0;
  }
  .body.dim {
    opacity: 0.55;
  }
  .tablewrap {
    min-height: 0;
    overflow: auto;
    border: 1px solid var(--line);
    border-radius: 4px;
  }
  table {
    width: 100%;
    border-collapse: collapse;
    font-size: 13px;
  }
  th {
    position: sticky;
    top: 0;
    z-index: 1;
    padding: 5px 8px;
    text-align: left;
    font-weight: 600;
    color: var(--muted);
    background: var(--panel);
    border-bottom: 1px solid var(--line);
  }
  td {
    padding: 4px 8px;
    border-bottom: 1px solid var(--line);
    vertical-align: middle;
  }
  tbody tr {
    cursor: pointer;
    background: var(--row);
  }
  tbody tr:nth-child(even) {
    background: var(--row-alt);
  }
  tbody tr:hover {
    background: var(--hover);
  }
  tbody tr.focus {
    outline: 1px solid var(--accent);
    outline-offset: -1px;
  }
  tr.left td,
  tr.off td {
    color: var(--muted);
  }
  .r {
    text-align: right;
    white-space: nowrap;
  }
  .chk {
    width: 28px;
    text-align: center;
  }
  .chk input {
    padding: 0;
  }
  .name {
    max-width: 200px;
  }
  .op {
    white-space: nowrap;
  }
  .shift {
    display: block;
    font-size: 11px;
  }
  .rowbtn {
    display: block;
    width: 100%;
    padding: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    text-align: left;
    background: none;
    border: none;
    color: inherit;
    font: inherit;
    cursor: pointer;
  }
  .why {
    font-style: italic;
  }
  .mixchip {
    display: inline-block;
    min-width: 18px;
    padding: 0 5px;
    text-align: center;
    border-radius: 9px;
    background: var(--accent-soft);
    color: var(--accent);
  }
  .side {
    min-height: 0;
    overflow: auto;
  }
  .mixwarn {
    margin: 0;
    padding: 8px 10px;
    border: 1px solid var(--accent);
    border-radius: 4px;
    background: var(--accent-soft);
    max-height: 90px;
    overflow: auto;
  }
  footer {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }
  .sr {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip: rect(0 0 0 0);
  }
  @media (max-width: 900px) {
    .body {
      grid-template-columns: minmax(0, 1fr);
      overflow: auto;
    }
  }
</style>
