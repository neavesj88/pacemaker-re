<script lang="ts">
  import { session } from "$lib/state/session.svelte";
  // The choices of the case export that are policies rather than a particular export: relative
  // paths, copying the audio, how the copies are laid out and named, what to do with a file already
  // there, and whether to remember the folder. Shown in the export dialog and in the Settings view,
  // on a copy of the saved choices each time (the dialog saves them when an export starts).
  import type { ExportPrefs } from "$lib/api/cases";

  let { prefs = $bindable(), canReplace = true }: { prefs: ExportPrefs; canReplace?: boolean } = $props();
  // radio groups are matched by name across the whole page, so each copy of this form has its own
  const uid = $props.id();
</script>

<fieldset>
  <legend>Options</legend>
  <label class="opt">
    <input type="checkbox" bind:checked={prefs.relative_paths} disabled={prefs.format !== "m3u8"} />
    <span>
      <strong>Relative paths in the M3U8</strong>
      <span class="muted note">{prefs.format === "m3u8" ? "Each track's path is written relative to the playlist file, so the folder can be moved. Where the audio is on another drive than the playlist the full path is written, and the export preview says how many." : "Only for M3U8; the other formats always hold full paths."}</span>
    </span>
  </label>
  <label class="opt">
    <input type="checkbox" bind:checked={prefs.copy_audio} />
    <span>
      <strong>Copy the audio files too</strong>
      <span class="muted note">Copies each track's file from {session.isComputer ? 'the library' : 'the Pacemaker'} next to the playlists and points the lists at the copies. The files {session.place.on} are only read.</span>
    </span>
  </label>
  {#if prefs.copy_audio}
    <div class="sub2">
      <div class="grp">
        <span class="lbl">Where</span>
        <label class="inline"><input type="radio" name="{uid}-layout" value="shared" bind:group={prefs.audio_layout} /> One “Audio” folder for all (a track in several cases is copied once)</label>
        <label class="inline"><input type="radio" name="{uid}-layout" value="per_case" bind:group={prefs.audio_layout} /> A folder per case, each with its own copies</label>
      </div>
      <div class="grp">
        <span class="lbl">Names</span>
        <label class="inline"><input type="radio" name="{uid}-names" value="tags" bind:group={prefs.audio_names} /> “Artist - Title” from the tags</label>
        <label class="inline"><input type="radio" name="{uid}-names" value="device" bind:group={prefs.audio_names} /> The name the Pacemaker gave the file</label>
      </div>
    </div>
  {/if}
  <div class="grp">
    <span class="lbl">If a file is already in the folder</span>
    <label class="inline"><input type="radio" name="{uid}-conflict" value="skip" bind:group={prefs.conflict} /> Leave it as it is and do not write that one</label>
    <label class="inline"><input type="radio" name="{uid}-conflict" value="number" bind:group={prefs.conflict} /> Keep it and save the new one as “Name (2)”</label>
    {#if canReplace}
      <label class="inline"><input type="radio" name="{uid}-conflict" value="replace" bind:group={prefs.conflict} /> Replace it <span class="muted">(every file replaced is named below)</span></label>
    {/if}
  </div>
  <label class="opt">
    <input type="checkbox" bind:checked={prefs.remember} />
    <span>Remember this folder and format for next time <span class="muted">(kept in this app, not on the Pacemaker)</span></span>
  </label>
</fieldset>

<style>
  .inline {
    display: inline-flex;
    gap: 5px;
    align-items: baseline;
    margin-right: 12px;
    cursor: pointer;
  }
  .lbl {
    display: block;
    color: var(--muted);
    font-size: 12px;
    margin-bottom: 2px;
  }
  fieldset {
    margin: 8px 0;
    padding: 6px 10px;
    border: 1px solid var(--line);
    border-radius: 4px;
  }
  legend {
    padding: 0 4px;
    color: var(--muted);
    font-size: 12px;
  }
  .opt {
    display: flex;
    gap: 8px;
    align-items: flex-start;
    margin: 4px 0;
    cursor: pointer;
  }
  .opt input {
    margin-top: 3px;
  }
  .note {
    display: block;
    font-size: 12px;
  }
  .sub2 {
    margin: 2px 0 6px 24px;
  }
  .grp {
    margin: 6px 0;
  }
  .grp .inline {
    display: flex;
    margin-bottom: 2px;
  }
</style>
