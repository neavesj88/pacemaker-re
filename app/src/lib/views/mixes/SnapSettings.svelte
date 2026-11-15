<script lang="ts">
  // The mix editor's Snap setting (also in the toolbar): whether a dragged clip is lined up with a
  // neighbour's beat grid, and on what. Remembered between sessions.
  import { mixes } from "$lib/state/mixes.svelte";
  import type { SnapGrain } from "$lib/state/snapPrefs";
  import { snapPrefs } from "./snapPrefs.svelte";

  const p = $derived(snapPrefs.value);
</script>

<div class="field">
  <span class="lbl">Snap</span>
  <label class="check"><input type="checkbox" checked={p.on} onchange={(e) => mixes.setSnap({ on: e.currentTarget.checked })} /> line a dragged clip up with its neighbour's beat grid</label>
</div>
<div class="field">
  <label for="ss-grain">Snap to</label>
  <select id="ss-grain" value={p.grain} disabled={!p.on} onchange={(e) => mixes.setSnap({ grain: e.currentTarget.value as SnapGrain })}>
    <option value="beat">Beat</option>
    <option value="bar">Bar (downbeat on downbeat)</option>
    <option value="phrase">Phrase (8 bars)</option>
    <option value="section">Section boundaries</option>
  </select>
</div>
<div class="muted small">
  When a clip is dragged over, or up against, another one, it moves so the two beat grids line up at this grain, using the tracks' real beat grids (a clip at half or double the other's tempo lines up on the beats they share). Hold Alt while dragging to move freely. The Beat-align button does the same in one step for the selected clip. A clip without a beat grid is not moved by it.
</div>

<style>
  .small {
    font-size: 12px;
    margin: 4px 0;
  }
  .muted {
    color: var(--muted);
  }
</style>
