<script lang="ts">
  // The track list and CUE sheet settings: shown in the track list dialog and in the Settings view.
  // They live in `tracklist` (kept on this computer), so a change here is the change there.
  import { session } from "$lib/state/session.svelte";
  import { tracklist } from "$lib/state/tracklist.svelte";

  tracklist.ensureLoaded();

  function changed() {
    tracklist.save();
  }
</script>

<div class="grid">
  <label for="tl-stamp">A track starts</label>
  <select id="tl-stamp" bind:value={tracklist.options.stampAt} onchange={changed}>
    <option value="enters">when it enters (the crossfade begins)</option>
    <option value="crossfadeMid">in the middle of the crossfade</option>
  </select>
  <label for="tl-round">Seconds</label>
  <select id="tl-round" bind:value={tracklist.options.rounding} onchange={changed}>
    <option value="floor">round down (never after the real start)</option>
    <option value="nearest">round to the nearest second</option>
  </select>
  <label for="tl-close">YouTube, tracks under 10 s apart</label>
  <select id="tl-close" bind:value={tracklist.options.closeEntries} onchange={changed}>
    <option value="merge">join the later name to the earlier chapter</option>
    <option value="drop">leave the later track out</option>
  </select>
</div>
<label class="check"><input type="checkbox" bind:checked={tracklist.options.numbered} onchange={changed} /> Number the lines of the text list</label>
<label class="check"><input type="checkbox" bind:checked={tracklist.options.includeUnavailable} onchange={changed} /> List tracks whose audio is not {session.place.on} (they are silent in the export)</label>
<label class="check"><input type="checkbox" bind:checked={tracklist.options.csvBom} onchange={changed} /> CSV file starts with a byte-order mark (Excel needs it for accented names)</label>
<label class="check"><input type="checkbox" bind:checked={tracklist.writeCue} onchange={changed} /> Write a CUE sheet next to every audio export</label>
<div class="grid">
  <label for="tl-cueex">When that .cue file is already there</label>
  <select id="tl-cueex" bind:value={tracklist.cueIfExists} onchange={changed} disabled={!tracklist.writeCue}>
    <option value="ask">ask before replacing it</option>
    <option value="replace">replace it without asking</option>
  </select>
</div>
<div class="muted note">These settings are kept on this computer. Lists are text you copy or save; nothing here is written to the device.</div>

<style>
  .grid {
    display: grid;
    grid-template-columns: max-content 1fr;
    gap: 6px 12px;
    align-items: center;
    margin: 8px 0;
  }
  .grid label {
    color: var(--muted);
  }
  .check {
    display: block;
    margin: 4px 0;
  }
  .note {
    margin-top: 8px;
    font-size: 12px;
  }
</style>
