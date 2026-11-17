<script lang="ts">
  // The "need analysis" block at the foot of the sidebar (a fixed slot rendered by
  // shell/Sidebar.svelte). Tracks-owned.
  import { analysis } from "$lib/state/analysis.svelte";
  import { bg } from "$lib/state/bganalysis.svelte";
  import { waitingNote } from "$lib/views/analysis/bgLogic";
  import { bpmRange } from "$lib/state/bpmRange.svelte";
  import { rangeLabel } from "$lib/state/bpmRangeLogic";
  import { cases } from "$lib/state/cases.svelte";
  import { energy } from "$lib/state/energy.svelte";
  import { filters } from "$lib/state/filters.svelte";
  import { jobs } from "$lib/state/jobs.svelte";
  import { nav } from "$lib/state/nav.svelte";
  import { sections } from "$lib/state/sections.svelte";
  import { session } from "$lib/state/session.svelte";
  import Button from "$lib/ui/Button.svelte";
  import { analysisLabel, untried } from "./needsLabel";

  // Energy and sections the worker has not tried yet (never analysed or changed), for the kinds it makes by
  // itself. Tracks it tried and got nothing from (flat, too short, no grid) are not work left.
  const pending = $derived(
    (bg.settings.enabled && bg.settings.energy && energy.state ? untried(energy.state) : 0) +
      (bg.settings.enabled && bg.settings.sections && sections.state ? untried(sections.state) : 0),
  );
  const waiting = $derived(waitingNote(bg.status, pending));

  function toggleNeeds() {
    filters.onlyNeeds = !filters.onlyNeeds;
    if (filters.onlyNeeds) {
      nav.go("tracks");
      cases.close();
    }
  }
</script>

{#if session.needs.size > 0}
  <div class="stat warn">{session.needs.size} need analysis</div>
  <Button small active={filters.onlyNeeds} onclick={toggleNeeds}>{filters.onlyNeeds ? "Show all tracks" : "Show them"}</Button>
  <Button
    small
    onclick={() => jobs.askReanalyse()}
    disabled={!jobs.canWrite}
    title="Rewrite their beat grids from the audio"
  >Analyse {session.needs.size}…</Button>
{:else if waiting}
  <div class="stat warn" title="Other analysis continues in the background.">{waiting}</div>
{:else}
  <div class="stat">All tracks analysed</div>
{/if}
{#if bpmRange.outside.length > 0}
  <div class="stat warn">{bpmRange.outside.length} read outside {rangeLabel(bpmRange.lo)} BPM</div>
  <Button small onclick={() => bpmRange.openOffer()} title="Analysed with another BPM range. Re-analyse with the Settings range.">
    Review {bpmRange.outside.length}…
  </Button>
{/if}
{#if energy.state && energy.state.total > 0}
  <Button small onclick={() => energy.openPanel()} title="Estimate track intensity from the audio. Kept by this app only.">
    {analysisLabel("Energy", "Estimate energy", energy.state.ready, energy.state.total)}
  </Button>
{/if}
{#if sections.state && sections.state.total > 0}
  <Button small onclick={() => sections.openPanel()} title="Mark intro, drops and outro on the waveforms. Kept by this app only.">
    {analysisLabel("Sections", "Find sections", sections.state.ready, sections.state.total)}
  </Button>
{/if}
{#if analysis.offered.length > 0}
  <Button small onclick={() => analysis.askDetectKeys()} disabled={!jobs.canWrite} title="Detect keys for tracks without one">
    Detect {analysis.offered.length} key{analysis.offered.length === 1 ? "" : "s"}…
  </Button>
{/if}
{#if analysis.noKeyFound.length > 0}
  <div class="stat" title="No steady pitch found. Set a key by hand from the track menu.">
    {analysis.noKeyFound.length} track{analysis.noKeyFound.length === 1 ? " has" : "s have"} no key to find
  </div>
{/if}
