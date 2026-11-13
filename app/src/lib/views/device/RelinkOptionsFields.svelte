<script lang="ts">
  // The options of the missing-audio search (`relink.settings`, kept in the app's config folder):
  // shown in the search dialog and in the Settings view. Whoever shows them saves them
  // (`relink.saveSettings()`): the dialog when a search starts, Settings on its Save button.
  import { relink } from "./relink.svelte";

  const s = $derived(relink.settings);

  function num(e: Event, lo: number, hi: number, fallback: number): number {
    const v = Number((e.currentTarget as HTMLInputElement).value);
    return Number.isFinite(v) ? Math.min(hi, Math.max(lo, v)) : fallback;
  }
</script>

{#if s}
  <label class="opt">
    <input type="checkbox" bind:checked={s.use_tags} />
    Also match by title, artist and length (reads the tags of files whose name does not match)
  </label>
  <label class="opt indent">
    Length may differ by
    <input
      class="n"
      type="number"
      min="0"
      max="60"
      step="0.5"
      disabled={!s.use_tags}
      value={s.duration_tolerance_secs}
      onchange={(e) => (s.duration_tolerance_secs = num(e, 0, 60, 2))}
    />
    seconds
  </label>
  <label class="opt">
    <input type="checkbox" bind:checked={s.subfolders} />
    Search the folders inside it too
  </label>
  <label class="opt">
    A match already on the card (under .Pacemaker/Music):
    <select bind:value={s.card_files}>
      <option value="reuse">point the track at it where it is</option>
      <option value="standard">copy it to the name this app gives new files</option>
    </select>
  </label>
  <label class="opt">
    Tick matches at the start:
    <select bind:value={s.preselect}>
      <option value="none">none, I tick them myself</option>
      <option value="sure">the sure ones</option>
      <option value="likely">the sure and the likely ones</option>
    </select>
  </label>
  <label class="opt">
    Stop reading tags after
    <input class="n w" type="number" min="0" max="1000000" step="1000" value={s.max_tag_reads} onchange={(e) => (s.max_tag_reads = Math.round(num(e, 0, 1_000_000, 20_000)))} />
    files
  </label>
  <label class="opt">
    Stop listing after
    <input class="n w" type="number" min="100" max="1000000" step="1000" value={s.max_files} onchange={(e) => (s.max_files = Math.round(num(e, 100, 1_000_000, 100_000)))} />
    audio files
  </label>
{/if}

<style>
  .opt {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 6px;
    padding: 4px 0;
  }
  .indent {
    padding-left: 22px;
  }
  .n {
    width: 5em;
    background: var(--deep);
    color: var(--text);
    border: 1px solid var(--line-hi);
    border-radius: 4px;
    padding: 2px 6px;
    font: inherit;
  }
  .n.w {
    width: 7em;
  }
  select {
    background: var(--deep);
    color: var(--text);
    border: 1px solid var(--line-hi);
    border-radius: 4px;
    padding: 2px 6px;
    font: inherit;
    max-width: 100%;
  }
</style>
