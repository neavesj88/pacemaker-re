<script lang="ts">
  // The rules behind the "Suspect grid" filter: the tempo window for each genre (and for genres with
  // none), the confidence floor and the two numbers the derived confidence uses. Kept in the app's
  // own storage, not on the device; they are only used to flag tracks and to pick "Suggested fix".
  import type { SuspectSettings } from "$lib/api/grid";
  import { DEFAULT_SUSPECT_SETTINGS, formatGenreWindows, parseGenreWindows, validateSettings } from "$lib/state/batchLogic";
  import { suspects } from "$lib/state/gridSuspects.svelte";
  import Button from "$lib/ui/Button.svelte";
  import Modal from "$lib/ui/Modal.svelte";

  let tempoRule = $state(suspects.rules.tempo_rule);
  let fallbackMin = $state(String(suspects.rules.fallback_min));
  let fallbackMax = $state(String(suspects.rules.fallback_max));
  let genresText = $state(formatGenreWindows(suspects.rules.genres));
  let qualityRule = $state(suspects.rules.quality_rule);
  let floor = $state(String(suspects.rules.min_confidence_pct));
  let tolerance = $state(String(suspects.rules.steady_tolerance_pct));
  let coverage = $state(String(suspects.rules.coverage_target_pct));
  let auto = $state(suspects.auto);
  let error = $state("");

  const toNumber = (t: string) => (t.trim() === "" ? NaN : Number(t.trim().replace(",", ".")));

  function fill(r: SuspectSettings) {
    tempoRule = r.tempo_rule;
    fallbackMin = String(r.fallback_min);
    fallbackMax = String(r.fallback_max);
    genresText = formatGenreWindows(r.genres);
    qualityRule = r.quality_rule;
    floor = String(r.min_confidence_pct);
    tolerance = String(r.steady_tolerance_pct);
    coverage = String(r.coverage_target_pct);
    error = "";
  }

  function save() {
    const g = parseGenreWindows(genresText);
    if ("error" in g) {
      error = g.error;
      return;
    }
    const rules: SuspectSettings = {
      tempo_rule: tempoRule,
      fallback_min: toNumber(fallbackMin),
      fallback_max: toNumber(fallbackMax),
      genres: g.genres,
      quality_rule: qualityRule,
      min_confidence_pct: toNumber(floor),
      steady_tolerance_pct: toNumber(tolerance),
      coverage_target_pct: toNumber(coverage),
    };
    const problem = validateSettings(rules);
    if (problem) {
      error = problem;
      return;
    }
    suspects.save(rules, auto);
    suspects.editing = false;
  }
</script>

<Modal title="Suspect grid rules" onclose={() => (suspects.editing = false)}>
  <fieldset>
    <legend>
      <label><input type="checkbox" bind:checked={tempoRule} /> Flag a tempo outside its genre's window</label>
    </legend>
    <label class="row">
      Genres without a window use
      <input class="n" inputmode="decimal" bind:value={fallbackMin} aria-label="General window, lowest BPM" /> to
      <input class="n" inputmode="decimal" bind:value={fallbackMax} aria-label="General window, highest BPM" /> BPM
    </label>
    <label class="col">
      Genre windows, one per line (a track's genre text only has to contain the name; the longest name wins)
      <textarea rows="8" spellcheck="false" bind:value={genresText} placeholder="house 115-132"></textarea>
    </label>
    <p class="muted">When half or double the tempo fits the window, that is the suggested fix.</p>
  </fieldset>

  <fieldset>
    <legend>
      <label><input type="checkbox" bind:checked={qualityRule} /> Flag a grid that looks poorly analysed</label>
    </legend>
    <p class="muted">
      The analyser does not store a confidence, so one is worked out from the stored grid: how many gaps between beat lines are steady,
      and how much of the track the grid covers.
    </p>
    <label class="row">Flag below <input class="n" inputmode="decimal" bind:value={floor} aria-label="Confidence floor in per cent" /> % confidence</label>
    <label class="row">A gap is steady within <input class="n" inputmode="decimal" bind:value={tolerance} aria-label="Steady gap tolerance in per cent" /> % of the usual gap</label>
    <label class="row">The grid should cover <input class="n" inputmode="decimal" bind:value={coverage} aria-label="Coverage target in per cent" /> % of the track</label>
  </fieldset>

  <label class="row"><input type="checkbox" bind:checked={auto} /> Check the library whenever it loads (reads every track's beat grid)</label>
  {#if error}<p class="err" role="alert">{error}</p>{/if}

  {#snippet actions()}
    <Button variant="ghost" onclick={() => fill(DEFAULT_SUSPECT_SETTINGS)}>Reset to defaults</Button>
    <Button onclick={() => (suspects.editing = false)}>Cancel</Button>
    <Button variant="primary" onclick={save}>Save</Button>
  {/snippet}
</Modal>

<style>
  fieldset {
    margin: 10px 0 0;
    padding: 8px 10px 10px;
    border: 1px solid var(--line);
    border-radius: 6px;
  }
  legend {
    padding: 0 4px;
    font-weight: 600;
  }
  .row {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 6px;
    margin-top: 8px;
  }
  .col {
    display: flex;
    flex-direction: column;
    gap: 4px;
    margin-top: 8px;
  }
  .n {
    width: 64px;
  }
  textarea {
    width: 100%;
    box-sizing: border-box;
    font-family: inherit;
    resize: vertical;
  }
  p {
    margin: 8px 0 0;
  }
  .err {
    color: var(--bad-text);
    font-size: 12px;
  }
</style>
