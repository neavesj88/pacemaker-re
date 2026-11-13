<script lang="ts">
  // Export audio: which part of the mix, how loud, then the file. Everything shown here is computed
  // by the same backend code the export runs (the range by `export_range_resolve`, the gains by
  // `export_loudness_analyse`), and the editor's preview plays with the analysed gains, so what is
  // measured and heard is what is written. The settings are kept on this computer; nothing here is
  // written to the device.
  import { exportCancel } from "$lib/api/export";
  import { clockText, durationText, estimateBytes, gainText, lufsText, peakText, sizeText } from "$lib/state/exportprefs";
  import { mixExport as X } from "$lib/state/mixexport.svelte";
  import { mixes } from "$lib/state/mixes.svelte";
  import { toast } from "$lib/state/toast.svelte";
  import Button from "$lib/ui/Button.svelte";
  import Modal from "$lib/ui/Modal.svelte";
  import { clipTitle, errText } from "./actions";
  import ExportEndAt from "./ExportEndAt.svelte";
  import ExportFileFields from "./ExportFileFields.svelte";
  import ExportLimitFields from "./ExportLimitFields.svelte";
  import ExportLoudnessFields from "./ExportLoudnessFields.svelte";
  import ExportTagFields from "./ExportTagFields.svelte";
  import { exportAudio } from "./flow";
  import { SR, mmss } from "./layout";
  import { preview } from "./preview.svelte";
  import { mv } from "./view.svelte";

  const doc = $derived(mixes.doc);

  // closing the mix closes the dialog
  $effect(() => {
    if (!mixes.doc) X.open = false;
  });

  /** The clips an export can start or end a range with: the ones the export plays, in timeline order. */
  const heard = $derived(
    (doc?.clips ?? [])
      .filter((c) => !c.clipBypassed && c.visibleEnd > c.visibleStart && c.visibleEnd > 0)
      .sort((a, b) => a.visibleStart - b.visibleStart || a.visibleEnd - b.visibleEnd),
  );
  const label = (c: (typeof heard)[number], i: number) => `${i + 1}. ${clipTitle(c)} (${mmss(c.visibleStartSec)})`;
  const titleOf = (id: string) => clipTitle(doc?.clips.find((c) => c.id === id));

  // ask the backend what the choice covers whenever it, the mix or the settings that matter change
  $effect(() => {
    if (!X.open) return;
    void [X.mode, X.fromText, X.toText, X.clipA, X.clipB, X.prefs.endAt, X.prefs.edgeFadeMs, mixes.doc];
    void X.refreshInfo();
  });

  // show the chosen part on the timeline while the dialog is open
  $effect(() => {
    const i = X.open ? X.info : null;
    mv.rangeBand = i && !i.whole ? { from: Math.round(i.fromSecs * SR), to: Math.round(i.toSecs * SR) } : null;
    return () => (mv.rangeBand = null);
  });

  /** The stretch covered by the clips selected in the editor (seconds), or null when none is heard. */
  const selectedSpan = $derived.by(() => {
    const picked = heard.filter((c) => mixes.selected.includes(c.id));
    if (picked.length === 0) return null;
    return { from: Math.min(...picked.map((c) => c.visibleStartSec)), to: Math.max(...picked.map((c) => c.visibleEndSec)) };
  });

  function takeMarked() {
    if (!mv.marked) return;
    X.fromText = clockText(mv.marked.from / SR);
    X.toText = clockText(mv.marked.to / SR);
  }

  function takeSelected() {
    if (!selectedSpan) return;
    X.fromText = clockText(selectedSpan.from);
    X.toText = clockText(selectedSpan.to);
  }

  function pickMode(m: "whole" | "time" | "clips") {
    X.mode = m;
    if (m === "time" && !X.fromText) {
      if (mv.marked) takeMarked();
      else {
        X.fromText = clockText(mv.playhead / SR);
        X.toText = clockText(doc?.lengthSec ?? 0);
      }
    }
    if (m === "clips" && (!X.clipA || !X.clipB) && heard.length > 0) {
      X.clipA = heard[0].id;
      X.clipB = heard[heard.length - 1].id;
    }
  }

  function changed() {
    X.save();
  }

  function close() {
    preview.pause();
    X.open = false;
  }

  const sizeGuess = $derived(X.info ? estimateBytes(X.prefs, X.info.durationSecs) : null);

  async function exportNow() {
    close();
    await exportAudio();
  }

  async function playRange() {
    if (preview.playing) {
      preview.pause();
      return;
    }
    const i = X.info;
    if (!i) return;
    try {
      await preview.playRange(i.fromSecs, i.toSecs);
    } catch (e) {
      toast.show(`Could not preview: ${errText(e)}`, "bad", 7000);
    }
  }

  const r = $derived(X.report);
  const blocked = $derived(!!X.problem || !!X.infoError || !X.info || mv.exporting || X.analysing || !doc || doc.clips.length === 0);
</script>

{#if X.open && doc}
  <div class="wide">
    <Modal title="Export audio" onclose={close}>
      <fieldset>
        <legend>Part of the mix</legend>
        <label class="opt"><input type="radio" name="xr-mode" checked={X.mode === "whole"} onchange={() => pickMode("whole")} /> The whole mix</label>
        <label class="opt"><input type="radio" name="xr-mode" checked={X.mode === "time"} onchange={() => pickMode("time")} /> A time range</label>
        {#if X.mode === "time"}
          <div class="row indent">
            <label for="xr-from">From</label>
            <input id="xr-from" class="time" type="text" bind:value={X.fromText} spellcheck="false" autocomplete="off" aria-label="Start of the range" />
            <Button small onclick={() => (X.fromText = clockText(mv.playhead / SR))} title="Use the playhead position">playhead</Button>
            <label for="xr-to">to</label>
            <input id="xr-to" class="time" type="text" bind:value={X.toText} spellcheck="false" autocomplete="off" aria-label="End of the range" />
            <Button small onclick={() => (X.toText = clockText(mv.playhead / SR))} title="Use the playhead position">playhead</Button>
            <span class="muted small">times on the ruler, like 12:05.5</span>
          </div>
          <div class="row indent">
            <Button small onclick={takeMarked} disabled={!mv.marked} title="Use the stretch marked by Shift-dragging the ruler">use the marked stretch</Button>
            <Button small onclick={takeSelected} disabled={!selectedSpan} title="Use the stretch from the start of the first selected track to the end of the last">use the selected tracks</Button>
            <span class="muted small">close this window and Shift-drag on the ruler to mark a stretch, or select tracks in the editor</span>
          </div>
        {/if}
        <label class="opt"><input type="radio" name="xr-mode" checked={X.mode === "clips"} onchange={() => pickMode("clips")} /> From one track to another</label>
        {#if X.mode === "clips"}
          <div class="row indent">
            <label for="xr-a">From</label>
            <select id="xr-a" bind:value={X.clipA}>
              {#each heard as c, i (c.id)}<option value={c.id}>{label(c, i)}</option>{/each}
            </select>
            <label for="xr-b">to the end of</label>
            <select id="xr-b" bind:value={X.clipB}>
              {#each heard as c, i (c.id)}<option value={c.id}>{label(c, i)}</option>{/each}
            </select>
          </div>
          <div class="indent"><ExportEndAt /></div>
        {/if}

        {#if X.infoError}
          <div class="bad" role="alert">{X.infoError}</div>
        {:else if X.info}
          <div class="info" aria-live="polite">
            {#if X.info.whole}
              The whole mix: {clockText(X.info.fromSecs)} to {clockText(X.info.toSecs)} ({durationText(X.info.durationSecs)}), {X.info.clipIds.length} track{X.info.clipIds.length === 1 ? "" : "s"}.
            {:else}
              Exports {clockText(X.info.fromSecs)} to {clockText(X.info.toSecs)} of the mix ({durationText(X.info.durationSecs)}), {X.info.clipIds.length} track{X.info.clipIds.length === 1 ? "" : "s"} heard:
              {X.info.clipIds.map(titleOf).join(", ")}.
              <div class="muted small">Effects start fresh at the cut: the tail of a reverb or echo from before the start is not in this part, and a tool already running at the start begins from its start.</div>
            {/if}
          </div>
        {/if}
      </fieldset>

      <fieldset>
        <legend>Loudness</legend>
        <ExportLoudnessFields />
        <div class="muted small">
          The gain is applied before the master limiter, which still keeps the peaks under -0.2 dBFS. A target the limiter would have to fight for is missed, and the result says by how much.
          Levelling measures each track as the mix plays it (tools and gain envelope included, crossfades not).
        </div>

        <div class="row">
          {#if X.analysing}
            <span class="busy">Measuring… {Math.round(X.analysePct * 100)}%</span>
            <Button small onclick={() => void exportCancel()}>Cancel</Button>
          {:else}
            <Button onclick={() => void X.analyse()} disabled={blocked} title="Measure the loudness of this part of the mix, and find the gains the export will apply. Nothing is written.">
              Measure loudness
            </Button>
            <span class="muted small">{X.active ? "finds the gains the export will use" : "just reads the level of the mix"}</span>
          {/if}
        </div>

        {#if r}
          <div class="report" class:stale={!X.current}>
            <div class="rhead">
              Measured {clockText(r.fromSecs)} to {clockText(r.toSecs)}
              {#if !X.current}<span class="badge" title="The mix, the range or the settings changed since this was measured.">out of date: measure again</span>{/if}
            </div>
            <table aria-label="Loudness readings">
              <thead><tr><th></th><th class="n">Loudness</th><th class="n">True peak</th></tr></thead>
              <tbody>
                <tr><td>Before the limiter{r.clips.length ? ", tracks levelled" : ""}</td><td class="n num">{lufsText(r.beforeLimiter?.lufs)}</td><td class="n num">{peakText(r.beforeLimiter?.truePeakDb)}</td></tr>
                <tr><td>After the limiter</td><td class="n num">{lufsText(r.afterLimiter?.lufs)}</td><td class="n num">{peakText(r.afterLimiter?.truePeakDb)}</td></tr>
              </tbody>
            </table>
            <div class="muted small">True peak is read at 4 times the sample rate and is accurate to about 0.3 dB.</div>
            <div class="gain">
              {#if r.targetLufs !== null}
                Master gain <b>{gainText(r.masterGainDb)}</b> to reach {lufsText(r.targetLufs)}.
                {#if r.predictedPeakDb !== null}Peaks then reach about {peakText(r.predictedPeakDb)} before the limiter.{/if}
                {#if r.limiterWillAct}<span class="warn">The limiter will work, so the file ends up quieter than the target.</span>{/if}
              {:else}
                No target: the master gain stays at 0.0 dB.
              {/if}
            </div>
            {#each r.notes as n (n)}<div class="note-warn">{n}</div>{/each}
            {#if r.clips.length > 0}
              <div class="tablewrap">
                <table aria-label="Track levelling">
                  <thead><tr><th class="idx">#</th><th>Track</th><th class="n">Loudness</th><th class="n">Gain</th></tr></thead>
                  <tbody>
                    {#each r.clips as c, i (c.clipId)}
                      <tr title={c.note ?? ""}>
                        <td class="idx num">{i + 1}</td>
                        <td class="name">{titleOf(c.clipId)}{#if c.note}<span class="muted"> · {c.note}</span>{/if}</td>
                        <td class="n num">{lufsText(c.lufs)}</td>
                        <td class="n num">{gainText(c.gainDb)}</td>
                      </tr>
                    {/each}
                  </tbody>
                </table>
              </div>
              <div class="muted small">Levelled to {lufsText(r.clipReferenceLufs)}.</div>
            {/if}
          </div>
        {/if}

        {#if X.lastExport?.result}
          <div class="muted small">Last export measured {lufsText(X.lastExport.result.lufs)}, {peakText(X.lastExport.result.truePeakDb)} (before encoding).</div>
        {/if}
      </fieldset>

      <fieldset>
        <legend>File</legend>
        <ExportFileFields />
        <div class="muted small">
          {#if sizeGuess !== null}About {sizeText(sizeGuess)} for {durationText(X.info?.durationSecs ?? 0)} of audio. {/if}
          The format is chosen again in the save window; a file name ending in another format's extension uses that format.
        </div>
      </fieldset>

      <fieldset>
        <legend>Tags</legend>
        <ExportTagFields mixTitle={doc.name} />
        <div class="muted small">Tags are written into the exported file only; nothing is added to the device. These settings are kept on this computer.</div>
      </fieldset>

      <label class="check">
        <input type="checkbox" bind:checked={X.prefs.previewAtExportLevel} onchange={changed} />
        Play the editor's preview at the export level (needs a current measurement)
      </label>
      {#if X.previewBehind}
        <div class="muted small">Loudness settings are on, but the preview plays without them until you measure again.</div>
      {/if}

      <details class="settings">
        <summary>More settings</summary>
        <ExportLimitFields />
        <div class="muted small">These settings are kept on this computer. They change how the audio file is made; nothing is written to the device.</div>
      </details>

      {#if X.problem}<div class="bad" role="alert">{X.problem}</div>{/if}

      {#snippet actions()}
        <Button onclick={close} data-autofocus>Close</Button>
        {#if X.partial}
          <Button disabled={!X.info || preview.status === "rendering"} onclick={() => void playRange()} title="Play this part as the export will make it, at the measured level when there is a current measurement">
            {preview.playing ? "Stop" : preview.status === "rendering" ? "Rendering…" : "Preview this part"}
          </Button>
        {/if}
        <Button variant="primary" disabled={blocked} onclick={() => void exportNow()} title={X.active ? "Choose the file; the export measures first, then renders with the gains found" : "Choose the file and render"}>
          Choose file and export…
        </Button>
      {/snippet}
    </Modal>
  </div>
{/if}

<style>
  .wide :global(.modal) {
    width: 780px;
    max-height: 92vh;
    overflow: auto;
  }
  fieldset {
    border: 1px solid var(--line);
    border-radius: 6px;
    margin: 0 0 10px;
    padding: 8px 12px 10px;
  }
  legend {
    color: var(--muted);
    font-size: 12px;
    padding: 0 6px;
  }
  .opt {
    display: block;
    margin: 4px 0;
  }
  .row {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 8px;
    margin: 6px 0;
  }
  .row label {
    color: var(--muted);
    font-size: 12px;
  }
  .indent {
    margin-left: 22px;
  }
  .time {
    width: 92px;
  }
  .small {
    font-size: 12px;
  }
  .info {
    margin-top: 6px;
    font-size: 12px;
    color: var(--text);
  }
  .bad,
  .note-warn {
    background: var(--bad-bg);
    color: var(--bad-text);
    border-radius: 4px;
    padding: 6px 8px;
    font-size: 12px;
    margin: 6px 0;
  }
  .busy {
    color: var(--green);
  }
  .report {
    margin-top: 8px;
    border: 1px solid var(--line);
    border-radius: 6px;
    padding: 8px 10px;
    background: var(--deep);
  }
  .report.stale {
    opacity: 0.6;
  }
  .rhead {
    color: var(--muted);
    font-size: 12px;
    margin-bottom: 4px;
  }
  .badge {
    color: var(--cue);
    margin-left: 8px;
  }
  .gain {
    margin: 6px 0;
  }
  .warn {
    color: var(--cue);
  }
  table {
    width: 100%;
    border-collapse: collapse;
    font-size: 13px;
  }
  th {
    text-align: left;
    font-weight: 600;
    color: var(--muted);
    padding: 3px 8px;
    font-size: 12px;
  }
  td {
    padding: 3px 8px;
    border-top: 1px solid var(--line);
  }
  .n {
    text-align: right;
    width: 110px;
  }
  .idx {
    width: 30px;
  }
  .tablewrap {
    max-height: 170px;
    overflow: auto;
    border: 1px solid var(--line);
    border-radius: 4px;
    margin-top: 6px;
  }
  .name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 380px;
  }
  .check {
    display: block;
    margin: 6px 0;
  }
  .settings {
    margin-top: 8px;
    font-size: 12px;
  }
  .settings summary {
    cursor: pointer;
    color: var(--muted);
  }
</style>
