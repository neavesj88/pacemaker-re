<script lang="ts">
  // The "need analysis" block at the foot of the sidebar (a fixed slot rendered by
  // shell/Sidebar.svelte). Tracks-owned.
  import { cases } from "$lib/state/cases.svelte";
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
