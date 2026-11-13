<script lang="ts">
  // The "Sections" panel (coverage, run buttons, settings), opened from the sidebar. Always
  // mounted by Overlays.svelte; renders nothing until opened.
  import type { SectionsSettings } from "$lib/api/analysis";
  import { jobs } from "$lib/state/jobs.svelte";
  import { sections } from "$lib/state/sections.svelte";
  import { session } from "$lib/state/session.svelte";
  import Button from "$lib/ui/Button.svelte";
  import Modal from "$lib/ui/Modal.svelte";
  import { clampSetting, coverageText, KIND_LABEL, KINDS, kindColor, LIMITATIONS, LIMITS, PHRASE_CHOICES, SENSITIVITY_NAMES, skipHint, type NumericSetting } from "./sectionLogic";

  const s = $derived(sections.state);
  const lib = $derived(session.library);
  let confirmForget = $state(false);
  let forgotten = $state<number | null>(null);

  $effect(() => {
    if (!sections.panelOpen) {
      confirmForget = false;
      forgotten = null;
    }
  });

  async function save(patch: Partial<SectionsSettings>) {
    if (!s) return;
    await sections.saveSettings({ ...s.settings, ...patch });
  }

  function saveNumber(e: Event, key: NumericSetting) {
    const input = e.currentTarget as HTMLInputElement;
    const value = clampSetting(key, input.valueAsNumber);
    input.value = String(value); // show what was kept
    void save({ [key]: value });
  }

  async function forget() {
    forgotten = await sections.clear();
    confirmForget = false;
  }

  const runDisabled = $derived(!jobs.canWrite || sections.busy);
  const saved = $derived(s ? s.ready + s.flat + s.skipped + s.stale : 0);
</script>

{#if sections.panelOpen}
  <Modal title="Sections" onclose={() => sections.closePanel()}>
    {#if !lib || !s}
      <p class="muted">Open a library to analyse its tracks.</p>
    {:else}
      <div class="body">
      <p class="lead">{coverageText(s)}</p>
      <p class="muted small">
        Finds the intro, builds, drops, breakdowns and outro of each track from its beat grid and its audio, and snaps them to
        8, 16 or 32 bar phrases. They are shown as coloured markers on the waveforms and are kept by this app, per library:
        they do not appear on the device. Estimates, not facts.
      </p>
      <p class="legend" aria-label="Marker colours">
        {#each KINDS as k (k)}<span><i style:background={kindColor(k)}></i>{KIND_LABEL[k]}</span>{/each}
      </p>
      {#if s.problem}<p class="warn small" role="status">{s.problem}</p>{/if}
      {#if s.skip_reasons.length > 0}
        <ul class="skips">
          {#each s.skip_reasons as r (r.reason)}
            <li><b>{r.count}</b> {r.reason}{#if skipHint(r.reason)}: <span class="muted">{skipHint(r.reason)}</span>{/if}</li>
          {/each}
        </ul>
      {/if}

      <div class="run">
        <Button
          variant="primary"
          disabled={runDisabled || s.total === 0}
          title="Tracks never analysed, whose file changed, or whose beat grid was edited since. You will be asked to confirm."
          onclick={() => {
            sections.closePanel();
            sections.askAnalyse();
          }}
        >Detect new or changed…</Button>
        <Button
          disabled={runDisabled || s.total === 0}
          title="Measure every track again, even those that already have sections. You will be asked to confirm."
          onclick={() => {
            sections.closePanel();
            sections.askAnalyse(undefined, true);
          }}
        >Analyse all {s.total} again…</Button>
        {#if confirmForget}
          <span class="forget" role="alert">
            Forget the {saved} saved results? The audio and the device are not touched; the markers disappear until you analyse again.
            <Button small data-autofocus onclick={() => (confirmForget = false)}>Keep them</Button>
            <Button small variant="primary" onclick={forget}>Forget</Button>
          </span>
        {:else}
          <Button variant="ghost" disabled={jobs.busy || saved === 0} onclick={() => (confirmForget = true)}>Forget saved results…</Button>
        {/if}
      </div>
      {#if forgotten !== null}<p class="muted small" role="status">{forgotten} saved result{forgotten === 1 ? "" : "s"} forgotten.</p>{/if}

      <h3>Settings</h3>
      <div class="settings">
        <label>
          Sensitivity
          <select value={s.settings.sensitivity} onchange={(e) => save({ sensitivity: Number(e.currentTarget.value) })}>
            {#each [1, 2, 3, 4, 5] as n (n)}<option value={n}>{n} - {SENSITIVITY_NAMES[n]}</option>{/each}
          </select>
          <span class="muted">higher finds smaller changes (and more false ones)</span>
        </label>
        <label>
          Shortest section
          <input type="number" min={LIMITS.min_section_bars.min} max={LIMITS.min_section_bars.max} step="1" value={s.settings.min_section_bars} onchange={(e) => saveNumber(e, "min_section_bars")} />
          bars
        </label>
        <label>
          Phrase length
          <select value={s.settings.phrase_bars} onchange={(e) => save({ phrase_bars: Number(e.currentTarget.value) })}>
            {#each PHRASE_CHOICES as n (n)}<option value={n}>{n === 0 ? "Automatic (per track)" : `${n} bars`}</option>{/each}
          </select>
        </label>
        <fieldset>
          <legend>Phrases start</legend>
          <label class="radio">
            <input type="radio" name="sections-origin" checked={s.settings.origin === "first_downbeat"} onchange={() => save({ origin: "first_downbeat" })} />
            <span><b>At the first downbeat</b><br /><span class="muted">the usual case for dance tracks that start on a phrase</span></span>
          </label>
          <label class="radio">
            <input type="radio" name="sections-origin" checked={s.settings.origin === "best_fit"} onchange={() => save({ origin: "best_fit" })} />
            <span><b>Where the track's boundaries fit best</b><br /><span class="muted">for tracks with a pickup or a few bars of lead-in; moves only for a clear gain</span></span>
          </label>
        </fieldset>
        <label>
          Snap a boundary to the phrase grid if within
          <input type="number" min={LIMITS.snap_tolerance_bars.min} max={LIMITS.snap_tolerance_bars.max} step="1" value={s.settings.snap_tolerance_bars} onchange={(e) => saveNumber(e, "snap_tolerance_bars")} />
          bars
        </label>
        <label>
          No sections for tracks under
          <input type="number" min={LIMITS.min_bars.min} max={LIMITS.min_bars.max} step="1" value={s.settings.min_bars} onchange={(e) => saveNumber(e, "min_bars")} />
          bars
        </label>
        <label>
          Tracks analysed at once
          <input type="number" min={LIMITS.threads.min} max={LIMITS.threads.max} step="1" value={s.settings.threads} onchange={(e) => saveNumber(e, "threads")} />
          <span class="muted">more is faster and uses more memory</span>
        </label>
        <label class="check">
          <input type="checkbox" checked={s.settings.show_markers} onchange={(e) => save({ show_markers: e.currentTarget.checked })} />
          Show section markers on waveforms
        </label>
        <p class="muted small">Changing a setting re-reads the saved measurements at once; nothing is decoded again.</p>
      </div>
      <details>
        <summary>How reliable is this?</summary>
        <p class="muted small">{LIMITATIONS}</p>
      </details>
      </div>
    {/if}
    {#snippet actions()}
      <Button data-autofocus onclick={() => sections.closePanel()}>Close</Button>
    {/snippet}
  </Modal>
{/if}

<style>
  /* the panel is taller than a small window: the settings scroll, the title and Close stay */
  .body {
    max-height: calc(100vh - 190px);
    overflow-y: auto;
    padding-right: 6px;
  }
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
  .legend {
    display: flex;
    flex-wrap: wrap;
    gap: 4px 12px;
    margin: 6px 0;
    font-size: 12px;
  }
  .legend span {
    display: inline-flex;
    align-items: center;
    gap: 5px;
  }
  .legend i {
    display: inline-block;
    width: 10px;
    height: 10px;
    border-radius: 2px;
  }
  .skips {
    margin: 4px 0;
    padding-left: 18px;
    font-size: 12px;
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
  details {
    margin-top: 12px;
  }
  summary {
    cursor: pointer;
    font-size: 12px;
  }
</style>
