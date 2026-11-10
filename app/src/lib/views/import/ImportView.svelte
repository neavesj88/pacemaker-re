<script lang="ts">
  import { untrack } from "svelte";
  import { jobs } from "$lib/state/jobs.svelte";
  import { session } from "$lib/state/session.svelte";
  import Button from "$lib/ui/Button.svelte";
  import ImportPreview from "./ImportPreview.svelte";
  import { interop } from "./interop.svelte";

  const lib = $derived(session.library);

  // what the list exports cover: "library", "case:<id>" or "mix:<id>"
  let scope = $state("library");
  $effect(() => {
    // a case or mix that no longer exists (another library, or deleted) falls back to the library
    const [kind, id] = scope.split(":");
    const list = kind === "case" ? lib?.cases : kind === "mix" ? lib?.mixes : null;
    if (kind !== "library" && !list?.some((c) => String(c.id) === id)) scope = "library";
  });

  function target() {
    const [kind, id] = scope.split(":");
    if (kind === "case" || kind === "mix") {
      const c = (kind === "case" ? lib?.cases : lib?.mixes)?.find((x) => String(x.id) === id);
      return { where: { scope: kind, id: Number(id) } as const, name: c?.name ?? kind };
    }
    return { where: { scope: "library" } as const, name: "Library" };
  }

  function exportList(format: "m3u8" | "csv") {
    const t = target();
    interop.export(format, t.where, t.name);
  }

  // After an add or an import finishes the library has changed: re-read the preview so it shows
  // what is left to do.
  $effect(() => {
    const done = jobs.job?.result;
    if (done) untrack(() => interop.refresh());
  });
</script>

<div class="import">
  <section class="card">
    <h3>Music files</h3>
    <p class="muted">Drop files or folders anywhere in the window, or pick them. Each is analysed, copied to the device and added with its beat grid.</p>
    <Button variant="primary" onclick={() => jobs.pickFiles()} disabled={!jobs.canWrite}>Add tracks…</Button>
  </section>

  <section class="card">
    <h3>rekordbox</h3>
    <p class="muted">rekordbox.xml: ratings and playlists become ratings and cases (previewed first); cues and grids are not imported yet. Export writes tracks with their tempo grids, ratings, keys and cases.</p>
    <div class="row">
      <Button onclick={() => interop.pick("Choose a rekordbox XML export")} disabled={!lib || interop.loading}>Import XML…</Button>
      <Button onclick={() => interop.export("rekordbox", { scope: "library" }, "rekordbox")} disabled={!lib || interop.exporting}>Export XML…</Button>
    </div>
  </section>

  <section class="card">
    <h3>iTunes / Music</h3>
    <p class="muted">Library.xml: ratings (0-100 become 0-5 stars) and playlists become cases.</p>
    <Button onclick={() => interop.pick("Choose an iTunes / Music Library.xml")} disabled={!lib || interop.loading}>Import…</Button>
  </section>

  <section class="card">
    <h3>Playlists and lists</h3>
    <p class="muted">Save a case, a mix or the whole library as an M3U8 playlist or a CSV list.</p>
    <div class="row">
      <select bind:value={scope} disabled={!lib} aria-label="What to export">
        <option value="library">Whole library</option>
        {#if lib?.cases.length}
          <optgroup label="Cases">
            {#each lib.cases as c (c.id)}<option value={`case:${c.id}`}>{c.name}</option>{/each}
          </optgroup>
        {/if}
        {#if lib?.mixes.length}
          <optgroup label="Mixes">
            {#each lib.mixes as m (m.id)}<option value={`mix:${m.id}`}>{m.name}</option>{/each}
          </optgroup>
        {/if}
      </select>
      <Button onclick={() => exportList("m3u8")} disabled={!lib || interop.exporting}>M3U8…</Button>
      <Button onclick={() => exportList("csv")} disabled={!lib || interop.exporting}>CSV…</Button>
    </div>
  </section>

  <section class="card">
    <h3>Serato · Traktor</h3>
    <p class="muted">Importing crates and cues from Serato or Traktor is not built yet.</p>
  </section>

  {#if interop.error}<p class="err" role="alert">{interop.error}</p>{/if}
  {#if interop.loading && !interop.plan}<p class="muted wide">Reading the file…</p>{/if}
  <ImportPreview />
</div>

<style>
  .import {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(320px, 1fr));
    gap: 12px;
    padding: 14px;
    align-items: start;
  }
  .card {
    padding: 12px 14px;
    background: var(--panel);
    border: 1px solid var(--line);
    border-radius: 8px;
  }
  h3 {
    margin: 0 0 6px;
    font-size: 14px;
  }
  .row {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    align-items: center;
  }
  select {
    background: var(--panel);
    color: var(--text);
    border: 1px solid var(--line);
    border-radius: 4px;
    padding: 5px 8px;
    font: inherit;
    max-width: 100%;
  }
  .err {
    grid-column: 1 / -1;
    margin: 0;
    padding: 8px 12px;
    border-radius: 6px;
    background: var(--bad-bg);
    color: var(--bad-text);
  }
  .wide {
    grid-column: 1 / -1;
    margin: 0;
  }
</style>
