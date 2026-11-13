<script lang="ts">
  // The settings of the tempo tools (`vtempo.settings`, kept on this computer, every number with a
  // default): shown under the tempo panel and in the Settings view. Edited on a copy and saved
  // as a whole once they check out.
  import { toast } from "$lib/state/toast.svelte";
  import Button from "$lib/ui/Button.svelte";
  import { vtempo } from "./vtempo.svelte";
  import { DEFAULT_VTEMPO, validateVTempo, type VTempoSettings } from "./vtempoLogic";

  let draft = $state<VTempoSettings>({ ...vtempo.settings });
  let settingsError = $state("");
  const fields: { key: keyof VTempoSettings; label: string; unit: string; step: number; hint: string }[] = [
    { key: "section_tolerance_pct", label: "New section when the beat gap changes by", unit: "%", step: 0.1, hint: "Beats closer in tempo than this are one section." },
    { key: "section_min_beats", label: "Shortest section", unit: "beats", step: 1, hint: "A shorter run is folded into its neighbour, unless you split there." },
    { key: "drift_threshold_ms", label: "A beat is off by more than", unit: "ms", step: 1, hint: "Measured from the median offset of the whole grid." },
    { key: "drift_min_run", label: "Drift needs", unit: "beats in a row", step: 1, hint: "Fewer off beats in a row are not reported." },
    { key: "drift_max_gap", label: "A drift span may bridge", unit: "beats with no onset", step: 1, hint: "Beats with nothing near them inside a run do not end it." },
    { key: "onset_min_strength", label: "Ignore onsets weaker than", unit: "(flux deviations)", step: 0.5, hint: "Applies the next time the audio is checked. Higher ignores quieter hits." },
    { key: "drift_window_pct", label: "Match an onset within", unit: "% of a beat", step: 1, hint: "How far from a beat line an onset may be to belong to it." },
    { key: "drift_window_max_ms", label: "...and within at most", unit: "ms", step: 5, hint: "Whichever of the two windows is smaller applies." },
    { key: "follow_smooth_beats", label: "Follow the audio smoothed over", unit: "beats", step: 2, hint: "Each beat moves by the median correction of the beats around it." },
    { key: "grab_px", label: "Grab a beat line within", unit: "px", step: 1, hint: "How close the pointer must be to pick up a beat line." },
    { key: "section_list_max", label: "List at most", unit: "sections", step: 50, hint: "A longer list is cut off with a note." },
    { key: "tap_count", label: "Tap tempo averages the last", unit: "taps", step: 1, hint: "More taps give a steadier number." },
    { key: "tap_reset_secs", label: "Tap count restarts after a pause of", unit: "s", step: 0.5, hint: "" },
  ];

  function applySettings() {
    const bad = validateVTempo(draft);
    settingsError = bad;
    if (bad) return;
    vtempo.save(draft);
    toast.show("Tempo tool settings saved", "ok");
  }

  function resetSettings() {
    draft = { ...DEFAULT_VTEMPO };
    settingsError = "";
    vtempo.resetSettings();
  }

  const settingsChanged = $derived(JSON.stringify(draft) !== JSON.stringify(vtempo.settings));

  function setNum(key: keyof VTempoSettings, v: string) {
    const x = Number(v.replace(",", "."));
    (draft as Record<string, unknown>)[key] = Number.isFinite(x) ? x : NaN;
  }
</script>

<div class="fields">
  {#each fields as f (f.key)}
    <label title={f.hint}>
      <span>{f.label}</span>
      <input
        class="num"
        inputmode="decimal"
        value={String(draft[f.key])}
        oninput={(e) => setNum(f.key, e.currentTarget.value)}
        aria-label={f.label}
      />
      <span class="muted">{f.unit}</span>
    </label>
  {/each}
  <label class="check">
    <input type="checkbox" bind:checked={draft.auto_check} />
    <span>Check the grid against the audio when a track opens (reads the file, a few seconds)</span>
  </label>
</div>
{#if settingsError}<p class="err">{settingsError}</p>{/if}
<div class="row">
  <Button small variant="primary" disabled={!settingsChanged} onclick={applySettings}>Save settings</Button>
  <Button small onclick={resetSettings}>Reset to defaults</Button>
</div>

<style>
  .fields {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(360px, 1fr));
    gap: 6px 18px;
    margin: 10px 0;
  }

  .fields label {
    display: grid;
    grid-template-columns: 1fr 76px auto;
    gap: 8px;
    align-items: center;
  }

  .fields input.num {
    text-align: right;
  }

  .fields .check {
    grid-template-columns: auto 1fr;
    grid-column: 1 / -1;
  }
  .err {
    color: var(--bad);
    margin: 6px 0;
  }
  .row {
    display: flex;
    gap: 8px;
    margin: 8px 0;
  }
</style>
