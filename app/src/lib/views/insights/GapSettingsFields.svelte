<script lang="ts">
  // The gap map's settings (`gap.settings`, kept on this computer): shown over the map and in the
  // Settings view.
  import Button from "$lib/ui/Button.svelte";
  import { gap } from "./gapState.svelte";
  import { BAND_WIDTHS, type GapSettings } from "./gapLogic";

  /** Applies one edited setting, then puts the field back to the value now in effect: when the
   *  setting was clamped back to what it already was, Svelte has nothing to rewrite and the typed
   *  number would otherwise stay on screen. */
  function change(e: Event, key: keyof GapSettings) {
    const el = e.currentTarget as HTMLInputElement | HTMLSelectElement;
    const next = gap.update({ [key]: Number(el.value) });
    el.value = String(next[key]);
  }
</script>

<div class="fields">
  <label>
    BPM band
    <select value={gap.settings.bandWidth} onchange={(e) => change(e, "bandWidth")}>
      {#each BAND_WIDTHS as w (w)}<option value={w}>{w} BPM wide</option>{/each}
    </select>
  </label>
  <label>
    BPM range from
    <input type="number" min="20" max="300" step="1" value={gap.settings.bpmMin} onchange={(e) => change(e, "bpmMin")} />
  </label>
  <label>
    up to
    <input type="number" min="30" max="400" step="1" value={gap.settings.bpmMax} onchange={(e) => change(e, "bpmMax")} />
  </label>
  <label>
    Genres shown
    <input type="number" min="3" max="40" step="1" value={gap.settings.maxGenres} onchange={(e) => change(e, "maxGenres")} />
  </label>
  <label>
    Thin means fewer than
    <input type="number" min="1" max="20" step="1" value={gap.settings.thinBelow} onchange={(e) => change(e, "thinBelow")} />
    tracks
  </label>
  <label>
    Judge rows from
    <input type="number" min="1" max="500" step="1" value={gap.settings.minRowTracks} onchange={(e) => change(e, "minRowTracks")} />
    tracks
  </label>
  <Button small variant="ghost" onclick={() => gap.resetSettings()}>Reset to defaults</Button>
  <span class="muted note">Saved on this computer. The range is trimmed to a whole number of bands.</span>
  <span class="notice" role="status">{gap.notice}</span>
</div>

<style>
  .fields {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px 16px;
  }
  label {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  input {
    width: 64px;
  }
  .note,
  .notice {
    font-size: 12px;
  }
  .notice {
    color: var(--cue);
  }
</style>
