<script lang="ts">
  // The "Energy source" setting: which energy the app uses wherever it needs one number per track
  // (the search box and label filter, smart cases, the gap map, the set builder, the printable
  // setlist). The Tracks table keeps both columns whatever this says. Saved by the backend because
  // smart cases read it when their Pacemaker copy is rewritten.
  import { ENERGY_SOURCES, isEnergySource } from "$lib/state/energyResolve";
  import { energySource } from "$lib/state/energySource.svelte";

  let { compact = false }: { compact?: boolean } = $props();

  const help = $derived(ENERGY_SOURCES.find((s) => s.id === energySource.source)?.help ?? "");
  let busy = $state(false);

  async function change(value: string) {
    if (!isEnergySource(value) || busy) return;
    busy = true;
    await energySource.set(value);
    busy = false;
  }
</script>

<label class="src" class:compact title={help}>
  <span class="lbl">Energy source</span>
  <select value={energySource.source} disabled={busy} aria-label="Energy source" onchange={(e) => change(e.currentTarget.value)}>
    {#each ENERGY_SOURCES as s (s.id)}<option value={s.id}>{s.label}</option>{/each}
  </select>
</label>
{#if !compact}
  <p class="help muted">{help} Applies to the search box, the energy filter, smart cases, the gap map, the set builder and the printable setlist; the Tracks table always shows both columns.</p>
{/if}
{#if energySource.problem}<p class="warn" role="alert">{energySource.problem}</p>{/if}

<style>
  .src {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }
  .lbl {
    color: var(--muted);
    font-size: 12px;
  }
  .help {
    margin: 4px 0 0;
    font-size: 12px;
  }
  .warn {
    margin: 4px 0 0;
    padding: 4px 8px;
    background: var(--bad-bg);
    color: var(--bad-text);
    border-radius: 4px;
    font-size: 12px;
  }
</style>
