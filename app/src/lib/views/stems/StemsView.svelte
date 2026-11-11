<script lang="ts">
  import { session } from "$lib/state/session.svelte";
  import { stems } from "$lib/state/stems.svelte";
  import { volumeName } from "$lib/format";
  import MakeCard from "./MakeCard.svelte";
  import ResultCard from "./ResultCard.svelte";
  import SettingsCard from "./SettingsCard.svelte";
  import SetupCard from "./SetupCard.svelte";

  // Vocal and instrumental versions of tracks, made on this computer and saved in a folder here.
  // The Pacemaker is only read (the source audio); putting results on the card is a separate,
  // confirmed step in the result card.
  const lib = $derived(session.library);

  // read when the view opens and again whenever the library is re-read (a job may have finished)
  $effect(() => {
    void lib?.root;
    void stems.refresh();
  });
</script>

<div class="bar">
  <h2>Stems</h2>
  {#if lib}<span class="chip">{volumeName(lib.root)}</span>{/if}
  <span class="muted sub">Acapella and instrumental versions of your tracks, made offline on this computer</span>
</div>

<div class="cols">
  <div class="col main">
    <MakeCard />
    <ResultCard />
  </div>
  <div class="col side">
    <SetupCard />
    <SettingsCard />
  </div>
</div>

<style>
  .bar {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 7px 12px;
    border-bottom: 1px solid var(--line);
  }
  h2 {
    margin: 0 8px 0 0;
    font-size: 15px;
  }
  .chip {
    font-size: 11px;
    padding: 1px 7px;
    border-radius: 9px;
    background: color-mix(in srgb, var(--green) 10%, transparent);
    color: var(--green);
  }
  .sub {
    font-size: 12px;
  }
  .cols {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(400px, 1fr));
    gap: 14px;
    padding: 12px;
    align-items: start;
  }
  .col {
    display: flex;
    flex-direction: column;
    gap: 14px;
    min-width: 0;
  }
</style>
