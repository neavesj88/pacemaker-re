<script lang="ts">
  // The mix editor's View options, also in the editor's View menu (one copy of each choice, so both
  // places agree): how tall the clips are while zooming, the whole-mix strip and the section
  // markers. Following the playhead is the next entry. Remembered between sessions.
  import { sections } from "$lib/state/sections.svelte";
  import { mv } from "./view.svelte";
  import { viewPrefs } from "./viewPrefs.svelte";
  import type { ClipHeightMode } from "./viewPrefsLogic";

  const v = $derived(viewPrefs.value);
  const HEIGHTS: { id: ClipHeightMode; label: string }[] = [
    { id: "shrink", label: "Shrinks as you zoom out (default)" },
    { id: "full", label: "Always full height" },
  ];
</script>

<div class="field" role="radiogroup" aria-label="Clip height">
  <span class="lbl">Clip height</span>
  {#each HEIGHTS as h (h.id)}
    <label class="check"><input type="radio" name="mv-clip-height" value={h.id} checked={v.clipHeight === h.id} onchange={() => viewPrefs.set({ clipHeight: h.id })} /> {h.label}</label>
  {/each}
</div>
<div class="field">
  <span class="lbl">Whole-mix strip</span>
  <label class="check"><input type="checkbox" checked={v.overview} onchange={(e) => viewPrefs.set({ overview: e.currentTarget.checked })} /> show the whole mix in a thin strip above the ruler</label>
</div>
<div class="field">
  <span class="lbl">Sections</span>
  <label class="check"><input type="checkbox" checked={mv.sectionMarkers && sections.showMarkers} disabled={!sections.showMarkers} onchange={(e) => mv.setSectionMarkers(e.currentTarget.checked)} /> draw the detected sections on the clips</label>
</div>
<div class="muted small">
  Clip height shrinks as you zoom out, freeing room for the track table. Section markers also need "show markers on waveforms" in Sections settings.
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
