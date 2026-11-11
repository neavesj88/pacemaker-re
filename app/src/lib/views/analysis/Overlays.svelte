<script lang="ts">
  // Always-mounted part of the analysis area: keeps the energy estimates in step with the open
  // library, and shows the "Energy (auto)" panel (coverage, run buttons, settings) when asked.
  import { untrack } from "svelte";
  import type { EnergySettings } from "$lib/api/analysis";
  import { energy } from "$lib/state/energy.svelte";
  import { jobs } from "$lib/state/jobs.svelte";
  import { session } from "$lib/state/session.svelte";
  import Button from "$lib/ui/Button.svelte";
  import Modal from "$lib/ui/Modal.svelte";
  import { clampSetting, coverageText, LIMITS, scaleText, toAnalyse, type NumericSetting } from "./energyLogic";

  // The library object changes on every open and every in-place reload (after a job, a BPM
  // edit), and a BPM edit changes the score, so the numbers are read again each time.
  $effect(() => {
    session.library;
    untrack(() => energy.load());
  });

  const s = $derived(energy.state);
  const lib = $derived(session.library);
  let confirmForget = $state(false);
  let forgotten = $state<number | null>(null);

  $effect(() => {
    if (!energy.panelOpen) {
      confirmForget = false;
      forgotten = null;
    }
  });

  async function save(patch: Partial<EnergySettings>) {
    if (!s) return;
    await energy.saveSettings({ ...s.settings, ...patch });
  }

  function saveNumber(e: Event, key: NumericSetting) {
    const input = e.currentTarget as HTMLInputElement;
    const value = clampSetting(key, input.valueAsNumber);
    input.value = String(value); // show what was kept
    void save({ [key]: value });
  }

  async function forget() {
    forgotten = await energy.clear();
    confirmForget = false;
  }

  const runDisabled = $derived(!jobs.canWrite || energy.busy);
</script>

{#if energy.panelOpen}
  <Modal title="Energy (auto)" onclose={() => energy.closePanel()}>
    {#if !lib || !s}
      <p class="muted">Open a library to analyse its tracks.</p>
    {:else}
      <p class="lead">{coverageText(s)}</p>
      <p class="muted small">
        A 1 to 10 estimate worked out from the audio: how loud the track is over time, how dense its attacks are, how much
        sits below 150 Hz, and its tempo. It is kept by this app, per library, and does not appear on the device. It is
        separate from any energy rating you set yourself.
      </p>
      <p class="muted small">{scaleText(s)}</p>
      {#if s.note}<p class="warn small" role="status">{s.note}</p>{/if}
      {#if s.problem}<p class="warn small" role="status">{s.problem}</p>{/if}

      <div class="run">
        <Button
          variant="primary"
          disabled={runDisabled || toAnalyse(s) === 0}
          title="Only tracks never analysed, or whose file has changed since. You will be asked to confirm."
          onclick={() => {
            energy.closePanel();
            energy.askAnalyse();
          }}
        >Analyse {toAnalyse(s)} new or changed…</Button>
        <Button
          disabled={runDisabled || s.total === 0}
          title="Measure every track again, even those that already have an estimate. You will be asked to confirm."
          onclick={() => {
            energy.closePanel();
            energy.askAnalyse(undefined, true);
          }}
        >Analyse all {s.total} again…</Button>
        {#if confirmForget}
          <span class="forget" role="alert">
            Forget the {s.ready + s.skipped + s.stale} saved results? The audio and the device are not touched.
            <Button small data-autofocus onclick={() => (confirmForget = false)}>Keep them</Button>
            <Button small variant="primary" onclick={forget}>Forget</Button>
          </span>
        {:else}
          <Button variant="ghost" disabled={jobs.busy || s.ready + s.skipped + s.stale === 0} onclick={() => (confirmForget = true)}>Forget saved results…</Button>
        {/if}
      </div>
      {#if forgotten !== null}<p class="muted small" role="status">{forgotten} saved result{forgotten === 1 ? "" : "s"} forgotten.</p>{/if}

      <h3>Settings</h3>
      <div class="settings">
        <fieldset>
          <legend>Scale</legend>
          <label class="radio">
            <input type="radio" name="energy-scale" checked={s.settings.scale === "fit"} onchange={() => save({ scale: "fit" })} />
            <span><b>Fit to this library</b><br /><span class="muted">spread its tracks over 1 to 10, so a typical library uses the whole range</span></span>
          </label>
          <label class="radio">
            <input type="radio" name="energy-scale" checked={s.settings.scale === "fixed"} onchange={() => save({ scale: "fixed" })} />
            <span><b>Fixed scale</b><br /><span class="muted">the same number for a track in any library, even when the library is all one style</span></span>
          </label>
        </fieldset>
        <label>
          Energy 1 sits at percentile
          <input type="number" min={LIMITS.fit_low_pct.min} max={LIMITS.fit_low_pct.max} step="1" value={s.settings.fit_low_pct} disabled={s.settings.scale !== "fit"} onchange={(e) => saveNumber(e, "fit_low_pct")} />
          of the library
        </label>
        <label>
          Energy 10 sits at percentile
          <input type="number" min={LIMITS.fit_high_pct.min} max={LIMITS.fit_high_pct.max} step="1" value={s.settings.fit_high_pct} disabled={s.settings.scale !== "fit"} onchange={(e) => saveNumber(e, "fit_high_pct")} />
          of the library
        </label>
        <label>
          No estimate for tracks under
          <input type="number" min={LIMITS.min_secs.min} max={LIMITS.min_secs.max} step="1" value={s.settings.min_secs} onchange={(e) => saveNumber(e, "min_secs")} />
          seconds
        </label>
        <label>
          Tracks analysed at once
          <input type="number" min={LIMITS.threads.min} max={LIMITS.threads.max} step="1" value={s.settings.threads} onchange={(e) => saveNumber(e, "threads")} />
          <span class="muted">more is faster and uses more memory</span>
        </label>
        <p class="muted small">Changing a setting re-scales the saved measurements at once; nothing is decoded again.</p>
      </div>
    {/if}
    {#snippet actions()}
      <Button data-autofocus onclick={() => energy.closePanel()}>Close</Button>
    {/snippet}
  </Modal>
{/if}

<style>
  .lead {
    margin: 0 0 8px;
  }
  .small {
    font-size: 12px;
    margin: 6px 0;
  }
  .warn {
    color: var(--accent);
  }
  .run {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    align-items: center;
    margin: 12px 0 4px;
  }
  .forget {
    display: inline-flex;
    flex-wrap: wrap;
    gap: 8px;
    align-items: center;
    color: var(--bad-text);
    background: var(--bad-bg);
    padding: 4px 8px;
    border-radius: 4px;
  }
  h3 {
    margin: 14px 0 6px;
    font-size: 13px;
  }
  .settings {
    display: grid;
    gap: 8px;
  }
  .settings label {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    align-items: center;
  }
  .settings input[type="number"] {
    width: 64px;
  }
  .settings input:disabled {
    opacity: 0.5;
  }
  fieldset {
    margin: 0;
    padding: 0;
    border: none;
    display: grid;
    gap: 6px;
  }
  legend {
    padding: 0;
    margin-bottom: 4px;
  }
  .settings .radio {
    flex-wrap: nowrap;
    align-items: flex-start;
  }
  .settings .radio input {
    margin-top: 3px;
  }
</style>
