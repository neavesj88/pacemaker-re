<script lang="ts">
  // The VirtualDJ list export's saved choices (`vdj.options`, kept on this computer): shown in the
  // export dialog, where the folder and list name are known, and in the Settings view, where they are not.
  import { vdj } from "./vdj.svelte";

  let { folderHint = "", listName = "" }: { folderHint?: string; listName?: string } = $props();

  // remember the options (the dialog also remembers the folder it exports into)
  $effect(() => {
    JSON.stringify($state.snapshot(vdj.options));
    vdj.persist();
  });
</script>

<div class="opts">
  <label class="inline">
    <input type="checkbox" bind:checked={vdj.options.copy_audio} /> Copy the audio into the folder
  </label>
  <p class="muted small indent">
    {#if vdj.options.copy_audio}
      A file that is already in the Audio folder with the same name, size and first and last 64 kB is reused, never overwritten.
    {:else}
      The list points at the files where they are on the Pacemaker, so the card has to be plugged in at the same place when VirtualDJ
      opens the list.
    {/if}
  </p>
  {#if vdj.options.copy_audio}
    <div class="row sub">
      <label for="vdj-names">Name the copies</label>
      <select id="vdj-names" bind:value={vdj.options.name_files}>
        <option value="artist_title">Artist - Title</option>
        <option value="original">As named on the Pacemaker</option>
      </select>
    </div>
    <div class="row sub">
      <label for="vdj-base">Folder as VirtualDJ will see it</label>
      <input id="vdj-base" bind:value={vdj.options.path_base} placeholder={folderHint || "same as above"} spellcheck="false" />
    </div>
    <p class="muted small indent">
      Leave empty if the stick keeps the same drive letter or mount point. Otherwise write the full path it will have on the DJ's computer
      (for example <code>D:\Gigs\{listName.trim() || "Set"}</code>), because the list needs full paths.
    </p>
  {/if}
  <label class="inline"><input type="checkbox" bind:checked={vdj.options.ordered} /> Keep the order of the case (list setting “ordered”)</label>
  <label class="inline"><input type="checkbox" bind:checked={vdj.options.no_duplicates} /> Do not allow the same track twice (list setting “noDuplicates”)</label>
</div>

<style>
  .opts {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-top: 8px;
    flex-wrap: wrap;
  }
  .row > label:first-child {
    width: 190px;
    flex: none;
    color: var(--muted);
  }
  .row input:not([type]) {
    flex: 1;
    min-width: 200px;
  }
  .sub {
    margin-left: 22px;
  }
  .inline {
    display: inline-flex;
    align-items: flex-start;
    gap: 6px;
  }
  .inline input {
    margin-top: 3px;
  }
  .small {
    font-size: 12px;
  }
  .indent {
    margin: 0 0 0 22px;
  }
  p {
    margin: 6px 0;
  }
  code {
    overflow-wrap: anywhere;
  }
</style>
