<script lang="ts">
  // The "need analysis" block at the foot of the sidebar (a fixed slot rendered by
  // shell/Sidebar.svelte). Tracks-owned.
  import { analysis } from "$lib/state/analysis.svelte";
  import { cases } from "$lib/state/cases.svelte";
  import { energy } from "$lib/state/energy.svelte";
  import { filters } from "$lib/state/filters.svelte";
  import { jobs } from "$lib/state/jobs.svelte";
  import { nav } from "$lib/state/nav.svelte";
  import { session } from "$lib/state/session.svelte";
  import Button from "$lib/ui/Button.svelte";

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
    title="Rewrite the beat grid (.str) of these tracks from their audio on the device"
  >Analyse {session.needs.size}…</Button>
{:else}
  <div class="stat">All tracks analysed</div>
{/if}
{#if energy.state && energy.state.total > 0}
  <Button small onclick={() => energy.openPanel()} title="Estimate how intense each track is from its audio. Kept by this app only; nothing is written to the device.">
    Energy {energy.state.ready} / {energy.state.total}…
  </Button>
{/if}
{#if analysis.keyless.length > 0}
  <Button small onclick={() => analysis.askDetectKeys()} disabled={!jobs.canWrite} title="Work out the key from the audio, only for tracks that have none">
    Detect {analysis.keyless.length} key{analysis.keyless.length === 1 ? "" : "s"}…
  </Button>
{/if}
