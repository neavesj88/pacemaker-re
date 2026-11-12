<script lang="ts">
  // Export cases and smart cases to a folder: one file per case in the chosen format, optionally
  // with the audio copied beside the playlists and the M3U8 pointing at the copies.
  //
  // The list under "What will be written" is the backend's own plan (`cases_export_plan`), the
  // one the export runs from, so every file named here is a file written, and nothing else is.
  // A file that already exists is never touched unless "replace" is chosen, and then each one is
  // named in red above the button. Nothing is written to the Pacemaker.
  import { untrack } from "svelte";
  import { pickExportFolder } from "$lib/api/cases";
  import { caseExport } from "$lib/state/caseExport.svelte";
  import { cases } from "$lib/state/cases.svelte";
  import { confirmLabel, EXT, FORMAT_LABEL, fmtBytes, listRank, plural, planSummary, replaceList, rows, totals } from "$lib/state/exportLogic";
  import Button from "$lib/ui/Button.svelte";

  const ex = caseExport;
  const p = $derived(ex.prefs);
  const plan = $derived(ex.plan);
  const t = $derived(plan ? totals(plan) : null);
  const replaced = $derived(plan ? replaceList(plan) : []);
  const nothingChosen = $derived(ex.caseIds.size === 0 && ex.smartIds.size === 0);
  const table = $derived.by(() => {
    if (!plan) return [];
    // playlists first; within each, files with a problem or a conflict before the plain ones
    return rows(plan)
      .map((r, i) => ({ r, i }))
      .sort((a, b) => (a.r.kind === b.r.kind ? 0 : a.r.kind === "list" ? -1 : 1) || listRank(a.r.action) - listRank(b.r.action) || a.i - b.i)
      .map((x) => x.r);
  });
  const canStart = $derived(!!plan && !!t && (t.lists > 0 || t.audio > 0) && !ex.planning && ex.planCurrent && ex.stage === "setup");
  const smartOfCase = (id: number) => cases.smart.find((s) => s.device_case_id === id);
  const FORMATS = ["m3u8", "csv", "rekordbox"] as const;

  // the preview follows every choice; keystrokes in the folder box are settled before asking
  const key = $derived(JSON.stringify(ex.request));
  $effect(() => {
    void key;
    if (!ex.open || ex.stage !== "setup") return;
    const timer = setTimeout(() => untrack(() => ex.refreshPlan()), 250);
    return () => clearTimeout(timer);
  });

  async function browse() {
    const dir = await pickExportFolder(p.folder);
    if (dir) ex.prefs.folder = dir;
  }

  const pct = $derived(ex.total > 0 ? Math.min(100, Math.round((ex.done / ex.total) * 100)) : 0);
</script>

<svelte:window onkeydown={(e) => ex.open && e.key === "Escape" && ex.close()} />

{#if ex.open}
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
  <div class="scrim" onclick={() => ex.close()}>
    <div class="modal" role="dialog" aria-modal="true" aria-label="Export cases to a folder" tabindex="-1" onclick={(e) => e.stopPropagation()}>
      {#if ex.stage === "setup"}
        <h2>Export cases to a folder</h2>
        {#if ex.warning}<p class="warn" role="alert">{ex.warning}</p>{/if}

        <div class="pick">
          <div class="col">
            <div class="head">
              <b>Cases</b>
              <Button small variant="link" onclick={() => ex.setSelection(cases.infos.map((c) => c.id), [...ex.smartIds])}>All</Button>
              <Button small variant="link" onclick={() => ex.setSelection([], [...ex.smartIds])}>None</Button>
            </div>
            <ul class="checks" aria-label="Cases to export">
              {#each cases.infos as c (c.id)}
                {@const sc = smartOfCase(c.id)}
                <li>
                  <label>
                    <input type="checkbox" checked={ex.caseIds.has(c.id)} onchange={(e) => ex.toggleCase(c.id, e.currentTarget.checked)} />
                    <span class="nm">{c.name}</span>
                    <span class="muted">{plural(c.track_count, "track")}</span>
                  </label>
                  {#if sc && ex.caseIds.has(c.id) && ex.smartIds.has(sc.id)}<div class="sub muted">Pacemaker copy of the smart case “{sc.name}”, which is ticked too: this one will not be written again.</div>{/if}
                </li>
              {:else}
                <li class="muted">No cases.</li>
              {/each}
            </ul>
          </div>
          <div class="col">
            <div class="head">
              <b>Smart cases</b>
              <Button small variant="link" onclick={() => ex.setSelection([...ex.caseIds], cases.smart.map((s) => s.id))}>All</Button>
              <Button small variant="link" onclick={() => ex.setSelection([...ex.caseIds], [])}>None</Button>
            </div>
            <ul class="checks" aria-label="Smart cases to export">
              {#each cases.smart as s (s.id)}
                <li>
                  <label>
                    <input type="checkbox" checked={ex.smartIds.has(s.id)} onchange={(e) => ex.toggleSmart(s.id, e.currentTarget.checked)} />
                    <span class="nm">{s.name}</span>
                    {#if cases.smartErrors[s.id]}<span class="bad" title={cases.smartErrors[s.id]}>cannot run</span>{:else}<span class="muted">{plural(cases.smartCounts[s.id] ?? 0, "track")} now</span>{/if}
                  </label>
                </li>
              {:else}
                <li class="muted">No smart cases.</li>
              {/each}
            </ul>
            <p class="muted small">A smart case is written as the tracks it matches right now; the file cannot keep the rules.</p>
          </div>
        </div>

        <div class="row">
          <label class="grow">
            <span class="lbl">Folder</span>
            <input class="path" bind:value={ex.prefs.folder} placeholder="Choose a folder outside the Pacemaker" aria-label="Export folder" spellcheck="false" />
          </label>
          <Button onclick={browse}>Browse…</Button>
        </div>
        <div class="row">
          <span class="lbl">Format</span>
          {#each FORMATS as f (f)}
            <label class="inline"><input type="radio" name="fmt" value={f} bind:group={ex.prefs.format} /> {FORMAT_LABEL[f]} <span class="muted">.{EXT[f]}</span></label>
          {/each}
        </div>

        <fieldset>
          <legend>Options</legend>
          <label class="opt">
            <input type="checkbox" bind:checked={ex.prefs.relative_paths} disabled={p.format !== "m3u8"} />
            <span>
              <strong>Relative paths in the M3U8</strong>
              <span class="muted note">{p.format === "m3u8" ? "Each track's path is written relative to the playlist file, so the folder can be moved. Where the audio is on another drive than the playlist the full path is written, and the list below says how many." : "Only for M3U8; the other formats always hold full paths."}</span>
            </span>
          </label>
          <label class="opt">
            <input type="checkbox" bind:checked={ex.prefs.copy_audio} />
            <span>
              <strong>Copy the audio files too</strong>
              <span class="muted note">Copies each track's file from the Pacemaker next to the playlists and points the lists at the copies. The files on the Pacemaker are only read.</span>
            </span>
          </label>
          {#if p.copy_audio}
            <div class="sub2">
              <div class="grp">
                <span class="lbl">Where</span>
                <label class="inline"><input type="radio" name="layout" value="shared" bind:group={ex.prefs.audio_layout} /> One “Audio” folder for all (a track in several cases is copied once)</label>
                <label class="inline"><input type="radio" name="layout" value="per_case" bind:group={ex.prefs.audio_layout} /> A folder per case, each with its own copies</label>
              </div>
              <div class="grp">
                <span class="lbl">Names</span>
                <label class="inline"><input type="radio" name="names" value="tags" bind:group={ex.prefs.audio_names} /> “Artist - Title” from the tags</label>
                <label class="inline"><input type="radio" name="names" value="device" bind:group={ex.prefs.audio_names} /> The name the Pacemaker gave the file</label>
              </div>
            </div>
          {/if}
          <div class="grp">
            <span class="lbl">If a file is already in the folder</span>
            <label class="inline"><input type="radio" name="conflict" value="skip" bind:group={ex.prefs.conflict} /> Leave it as it is and do not write that one</label>
            <label class="inline"><input type="radio" name="conflict" value="number" bind:group={ex.prefs.conflict} /> Keep it and save the new one as “Name (2)”</label>
            <label class="inline"><input type="radio" name="conflict" value="replace" bind:group={ex.prefs.conflict} /> Replace it <span class="muted">(every file replaced is named below)</span></label>
          </div>
          <label class="opt">
            <input type="checkbox" bind:checked={ex.prefs.remember} />
            <span>Remember this folder and format for next time <span class="muted">(kept in this app, not on the Pacemaker)</span></span>
          </label>
        </fieldset>

        <div class="preview" class:stale={plan && !ex.planCurrent} aria-live="polite">
          <b>What will be written</b>{#if plan && !ex.planCurrent}<span class="muted"> · updating after your change, Export is off until it is done</span>{/if}
          {#if nothingChosen}
            <p class="muted">Tick at least one case or smart case.</p>
          {:else if (p.folder ?? "").trim() === ""}
            <p class="muted">Choose a folder to see what will be written.</p>
          {:else if ex.planError}
            <p class="warn" role="alert">{ex.planError}</p>
          {:else if plan}
            {#if planSummary(plan)}<p>{planSummary(plan)}.</p>{/if}
            {#each plan.notes as n}<p class="note2">{n}</p>{/each}
            {#if replaced.length > 0}
              <div class="danger" role="alert">
                <b>{plural(replaced.length, "existing file")} will be written over and cannot be got back:</b>
                <div class="names">{replaced.join(" · ")}</div>
              </div>
            {/if}
            <div class="tablewrap">
              <table>
                <thead><tr><th>File (inside the folder)</th><th>What happens</th><th>Holds</th></tr></thead>
                <tbody>
                  {#each table as r (r.kind + r.file)}
                    <tr class="a-{r.action}">
                      <td class="file">{r.file}</td>
                      <td>{r.text}</td>
                      <td class="muted">{r.detail}</td>
                    </tr>
                  {/each}
                </tbody>
              </table>
            </div>
            <p class="muted small">{plural(plan.lists.length, "playlist file")}{plan.audio.length > 0 ? ` and ${plural(plan.audio.length, "audio file")} (${fmtBytes(plan.copy_bytes)} to copy)` : ""}. Disk space is not checked beforehand; a copy that fails is reported and its partial file removed.</p>
          {:else if ex.planning}
            <p class="muted">Working it out…</p>
          {/if}
        </div>
        {#if ex.startError}<p class="warn" role="alert">{ex.startError}</p>{/if}

        <div class="actions">
          <Button onclick={() => ex.close()}>Cancel</Button>
          <Button variant="primary" disabled={!canStart} onclick={() => ex.start()} data-autofocus>{plan && ex.planCurrent ? confirmLabel(plan) : "Export"}</Button>
        </div>
      {:else if ex.stage === "running"}
        <h2>Exporting…</h2>
        <progress max={Math.max(ex.total, 1)} value={ex.done}></progress>
        <p><b>{ex.done} of {ex.total}</b> <span class="muted cur">{ex.current || "starting…"}</span></p>
        {#if ex.failures.length > 0}
          <ul class="fails">{#each ex.failures as f}<li><b>{f.label}</b>: {f.error}</li>{/each}</ul>
        {/if}
        <p class="muted small">Files already written stay in the folder if you stop. A copy in progress is removed.</p>
        <div class="actions">
          <Button onclick={() => ex.close()}>Hide</Button>
          <Button variant="primary" disabled={ex.cancelling} onclick={() => ex.cancel()}>{ex.cancelling ? "Stopping…" : "Stop"}</Button>
        </div>
      {:else}
        <h2>{ex.result?.error ? "Export failed" : ex.done < ex.total ? "Export stopped" : "Export finished"}</h2>
        {#if ex.result?.error}
          <p class="warn" role="alert">{ex.result.error}</p>
        {:else}
          <p>{ex.result?.summary}</p>
        {/if}
        {#if ex.failures.length > 0}
          <p class="warn">Not written:</p>
          <ul class="fails">{#each ex.failures as f}<li><b>{f.label}</b>: {f.error}</li>{/each}</ul>
        {/if}
        <div class="actions">
          <Button variant="primary" onclick={() => ex.close()} data-autofocus>Close</Button>
        </div>
      {/if}
    </div>
  </div>
{/if}

<style>
  .scrim {
    position: fixed;
    inset: 0;
    z-index: 30;
    display: flex;
    align-items: center;
    justify-content: center;
    background: rgba(0, 0, 0, 0.6);
  }
  .modal {
    width: 780px;
    max-width: 94vw;
    max-height: 92vh;
    overflow: auto;
    padding: 18px 20px;
    background: var(--panel);
    border: 1px solid var(--line-hi);
    border-radius: 8px;
  }
  h2 {
    margin: 0 0 8px;
    font-size: 16px;
  }
  p {
    margin: 4px 0 8px;
  }
  .small {
    font-size: 12px;
  }
  .warn {
    color: var(--bad-text);
  }
  .bad {
    color: var(--bad-text);
    font-size: 12px;
  }
  .pick {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 12px;
    margin-bottom: 10px;
  }
  .head {
    display: flex;
    gap: 6px;
    align-items: baseline;
    margin-bottom: 4px;
  }
  .checks {
    margin: 0;
    padding: 4px 6px;
    list-style: none;
    max-height: 140px;
    overflow: auto;
    background: var(--deep);
    border: 1px solid var(--line);
    border-radius: 4px;
  }
  .checks label {
    display: flex;
    gap: 6px;
    align-items: baseline;
    cursor: pointer;
  }
  .nm {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .sub {
    margin: 0 0 3px 22px;
    font-size: 11px;
  }
  .row {
    display: flex;
    gap: 10px;
    align-items: flex-end;
    margin: 8px 0;
    flex-wrap: wrap;
  }
  .row .grow {
    flex: 1;
    min-width: 260px;
    display: block;
  }
  .lbl {
    display: block;
    color: var(--muted);
    font-size: 12px;
    margin-bottom: 2px;
  }
  .row > .lbl {
    margin: 0;
  }
  .path {
    width: 100%;
    box-sizing: border-box;
    background: var(--deep);
    color: var(--text);
    border: 1px solid var(--line);
    border-radius: 4px;
    padding: 5px 8px;
    font: inherit;
  }
  .path:focus {
    outline: 1px solid var(--line-hi);
  }
  .inline {
    display: inline-flex;
    gap: 5px;
    align-items: baseline;
    margin-right: 12px;
    cursor: pointer;
  }
  fieldset {
    margin: 8px 0;
    padding: 6px 10px;
    border: 1px solid var(--line);
    border-radius: 4px;
  }
  legend {
    padding: 0 4px;
    color: var(--muted);
    font-size: 12px;
  }
  .opt {
    display: flex;
    gap: 8px;
    align-items: flex-start;
    margin: 4px 0;
    cursor: pointer;
  }
  .opt input {
    margin-top: 3px;
  }
  .note {
    display: block;
    font-size: 12px;
  }
  .sub2 {
    margin: 2px 0 6px 24px;
  }
  .grp {
    margin: 6px 0;
  }
  .grp .inline {
    display: flex;
    margin-bottom: 2px;
  }
  .preview.stale .tablewrap,
  .preview.stale .danger {
    opacity: 0.45;
  }
  .preview {
    margin: 10px 0 4px;
  }
  .note2 {
    font-size: 12px;
    color: var(--text);
    margin: 2px 0;
  }
  .danger {
    margin: 6px 0;
    padding: 6px 8px;
    background: var(--bad-bg);
    border: 1px solid var(--bad);
    border-radius: 4px;
    color: var(--bad-text);
  }
  .names {
    max-height: 70px;
    overflow: auto;
    font-size: 12px;
    margin-top: 3px;
  }
  .tablewrap {
    max-height: 230px;
    overflow: auto;
    background: var(--deep);
    border: 1px solid var(--line);
    border-radius: 4px;
  }
  table {
    width: 100%;
    border-collapse: collapse;
    font-size: 12px;
  }
  th {
    position: sticky;
    top: 0;
    background: var(--panel);
    text-align: left;
    padding: 3px 8px;
    color: var(--muted);
    font-weight: 500;
    border-bottom: 1px solid var(--line);
  }
  td {
    padding: 2px 8px;
    vertical-align: top;
    border-bottom: 1px solid var(--line);
  }
  tbody tr {
    content-visibility: auto;
    contain-intrinsic-size: auto 22px;
  }
  .file {
    word-break: break-all;
  }
  .a-replace td:nth-child(2),
  .a-missing td:nth-child(2),
  .a-blocked td:nth-child(2) {
    color: var(--bad-text);
  }
  .a-skip td,
  .a-same td {
    color: var(--muted);
  }
  .a-renamed td:nth-child(2) {
    color: var(--cue);
  }
  .actions {
    display: flex;
    gap: 8px;
    justify-content: flex-end;
    margin-top: 14px;
  }
  progress {
    width: 100%;
    height: 8px;
    accent-color: var(--accent);
  }
  .cur {
    margin-left: 8px;
    word-break: break-all;
  }
  .fails {
    margin: 6px 0 0;
    padding-left: 18px;
    color: var(--bad-text);
    max-height: 160px;
    overflow: auto;
    font-size: 12px;
  }
</style>
