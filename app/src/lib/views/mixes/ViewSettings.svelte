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
  Shrinks as you zoom out: zoomed in, the clips are as tall as the window allows; zooming out makes them shorter (down to a height where the name, section labels and crossfades can still be read), so the track table under the mix gets the room (it grows as the clips shrink). Turning the whole-mix strip on takes its height from the table, not from the clips. The waveform is drawn the same way, just shorter. Always full height keeps the zoomed-in height at every zoom. The whole-mix strip shows every clip, the overlaps and the playhead, with a box over the part shown below: click it to centre the view, drag the box to scroll. Sections on the clips also needs "show markers on waveforms" in the Sections panel's settings. These are also in the View menu in the mix editor.
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
