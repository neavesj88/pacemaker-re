<script lang="ts">
  // The print options of the setlist page, in parts so the setlist dialog and the Settings view show
  // the same controls on the same state (`setlist.options`, kept on this computer; whoever shows
  // these calls `setlist.persist()` when one changes). The dialog's per-setlist text (heading,
  // event, notes) is not a setting.
  import { setlist } from "./setlist.svelte";
  import { COLUMNS } from "./setlistLogic";

  let {
    part,
    kind = null,
    shown = null,
  }: {
    part: "time" | "columns" | "page";
    /** What is being printed; null where nothing is (Settings), which shows every option. */
    kind?: "case" | "mix" | null;
    /** The columns that have something to print, to mark the ones that would be left out. */
    shown?: Set<string> | null;
  } = $props();
</script>

{#if part === "time"}
  <label>Show
    <select bind:value={setlist.options.timeMode}>
      <option value="elapsed">Time since the start of the set</option>
      <option value="clock">Clock time</option>
    </select>
  </label>
{:else if part === "columns"}
  <div class="checks">
    {#each COLUMNS as c (c.id)}
      <label class="chk" title={c.id === "title" ? "The title is always printed" : undefined}>
        <input type="checkbox" bind:checked={setlist.options.columns[c.id]} disabled={c.id === "title"} />
        {c.label}
        {#if shown && setlist.options.columns[c.id] && !shown.has(c.id)}<span class="muted small">(empty, left out)</span>{/if}
      </label>
    {/each}
  </div>
  {#if kind === "mix"}<p class="muted small">BPM is the tempo stored for each track, not the speed it is stretched to in the mix.</p>{/if}
  <label class="chk"><input type="checkbox" bind:checked={setlist.options.hideEmpty} /> Leave out a column that is empty for every track</label>
  <label>Key as
    <select bind:value={setlist.options.keyNotation}>
      <option value="stored">Stored on the device</option>
      <option value="camelot">Camelot (8A)</option>
      <option value="name">Name (A minor)</option>
    </select>
  </label>
{:else}
  <div class="two">
    <label>Paper
      <select bind:value={setlist.options.paper}>
        <option value="auto">Printer default</option>
        <option value="a4">A4</option>
        <option value="letter">US Letter</option>
      </select>
    </label>
    <label>Text size
      <select bind:value={setlist.options.size}>
        <option value="normal">Normal</option>
        <option value="large">Large</option>
        <option value="xlarge">Extra large (for the booth)</option>
      </select>
    </label>
  </div>
  <label class="chk"><input type="checkbox" bind:checked={setlist.options.landscape} /> Landscape</label>
  <label class="chk"><input type="checkbox" bind:checked={setlist.options.zebra} /> Shade every other row</label>
  <label class="chk"><input type="checkbox" bind:checked={setlist.options.showTotals} /> Show the track count and total time</label>
  {#if kind !== "case"}
    <label class="chk"><input type="checkbox" bind:checked={setlist.options.includeMuted} /> Include muted clips{kind === "mix" ? "" : " (mixes)"}</label>
  {/if}
{/if}

<style>
  label {
    display: grid;
    gap: 2px;
    margin-top: 6px;
    font-size: 12px;
    color: var(--muted);
  }
  label.chk {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-top: 4px;
    color: var(--text);
    font-size: 13px;
  }
  .checks {
    display: grid;
    grid-template-columns: 1fr 1fr;
  }
  .two {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 8px;
  }
  input:not([type="checkbox"]),
  select {
    width: 100%;
    box-sizing: border-box;
    min-width: 0;
  }
  .small {
    font-size: 12px;
    margin: 2px 0 0;
  }
</style>
