<script lang="ts">
  // One track's "Energy (auto)" in the Tracks table: the number and a thin meter. Read-only on
  // purpose: it is an estimate, not a rating you set.
  import type { EnergyEntry } from "$lib/api/analysis";
  import { cellTitle, energyLabel, meterPercent } from "./energyLogic";

  let { entry }: { entry: EnergyEntry | null } = $props();
</script>

{#if entry}
  <span class="energy" title={cellTitle(entry)}>
    <span class="bar" aria-hidden="true"><i style:width="{meterPercent(entry.energy)}%"></i></span>
    <span class="n">{energyLabel(entry.energy)}</span>
  </span>
{/if}

<style>
  .energy {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    width: 100%;
    min-width: 0;
  }
  .bar {
    flex: 1;
    min-width: 12px;
    height: 4px;
    border-radius: 2px;
    background: var(--line);
    overflow: hidden;
  }
  i {
    display: block;
    height: 100%;
    background: var(--accent-ramp);
  }
  .n {
    flex: none;
    width: 2.4em;
    text-align: right;
    font-variant-numeric: tabular-nums;
  }
</style>
