<script lang="ts">
  // Which of the library's tag fields are written into the audio files (a saved setting of the
  // clean-up recipes): shown on the "Write tags to files" page and in the Settings view.
  import { untrack } from "svelte";
  import { cleanup } from "./cleanupState.svelte";
  import { fieldLabel } from "./tagwriteState.svelte";

  const FIELDS = [
    "title", "artist", "album_artist", "album", "composer", "genre", "label", "remixer", "producer", "comments", "year",
    "track_number", "number_of_tracks", "disc_number", "number_of_discs",
  ];

  let s = $derived(cleanup.settings);

  // The Tidy page saves the recipes as they change, but the Settings view has no Tidy page behind
  // it, so the choice is saved from here (scheduleSave does nothing when nothing changed).
  $effect(() => {
    void cleanup.signature;
    untrack(() => cleanup.scheduleSave());
  });

  function toggleField(f: string, on: boolean) {
    if (!s) return;
    s.write_fields = on ? FIELDS.filter((k) => k === f || s!.write_fields.includes(k)) : s.write_fields.filter((k) => k !== f);
  }
</script>

{#if s}
  <fieldset class="fields">
    <legend>Fields to write (saved)</legend>
    {#each FIELDS as f (f)}
      <label>
        <input type="checkbox" checked={s.write_fields.includes(f)} onchange={(e) => toggleField(f, e.currentTarget.checked)} />
        {fieldLabel(f)}
      </label>
    {/each}
  </fieldset>
{/if}

<style>
  .fields {
    display: flex;
    flex-wrap: wrap;
    gap: 2px 14px;
    border: 1px solid var(--line);
    border-radius: 6px;
    padding: 6px 12px 8px;
    margin: 6px 0;
    font-size: 12px;
  }
  .fields legend {
    color: var(--muted);
    padding: 0 4px;
  }
  .fields label {
    display: inline-flex;
    align-items: center;
    gap: 4px;
  }
</style>
