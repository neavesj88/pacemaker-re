<script lang="ts">
  import { untrack } from "svelte";
  import { cases } from "$lib/state/cases.svelte";
  import { jobs } from "$lib/state/jobs.svelte";
  import { session } from "$lib/state/session.svelte";
  import { smartSettings } from "$lib/state/smartSettings.svelte";
  import Button from "$lib/ui/Button.svelte";
  import ImportPreview from "./ImportPreview.svelte";
  import WatchFolders from "./WatchFolders.svelte";
  import WatchResults from "./WatchResults.svelte";
  import { interop } from "./interop.svelte";

  const lib = $derived(session.library);

  // what the list exports cover: "library", "case:<id>", "mix:<id>" or "smart:<id>"
  let scope = $state("library");
  $effect(() => {
    void smartSettings.ensure();
  });
  $effect(() => {
    // a case, mix or smart case that no longer exists (another library, or deleted) falls back to the library
    const [kind, id] = scope.split(":");
    const list = kind === "case" ? lib?.cases : kind === "mix" ? lib?.mixes : kind === "smart" ? cases.smart : null;
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
    const [kind, id] = scope.split(":");
    if (kind === "smart") {
      // a smart case is exported as what it matches right now (the cases area owns that export)
      const s = cases.smart.find((x) => String(x.id) === id);
      if (s) cases.exportSmart(s, format);
      return;
    }
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

  <WatchFolders />
  <WatchResults />

  <section class="card">
    <h3>rekordbox</h3>
    <p class="muted">rekordbox.xml: ratings, playlists and the first memory cue become ratings, cases and (if you choose) cue points, previewed first; beat grids are not imported. Export writes tracks with their tempo grids, ratings, keys and cases. Smart cases are written as ordinary playlists of the tracks they match now (rekordbox XML cannot hold rules).</p>
    <div class="row">
      <Button onclick={() => interop.pick("rekordbox")} disabled={!lib || interop.loading}>Import XML…</Button>
      <Button onclick={() => interop.export("rekordbox", { scope: "library" }, "rekordbox")} disabled={!lib || interop.exporting}>Export XML…</Button>
    </div>
    <label class="opt">
      <input type="checkbox" checked={smartSettings.value.export_smart} onchange={(e) => smartSettings.set({ export_smart: e.currentTarget.checked })} />
      Include smart cases{cases.smart.length ? ` (${cases.smart.length})` : ""}
    </label>
  </section>

  <section class="card">
    <h3>iTunes / Music</h3>
    <p class="muted">Library.xml: ratings (0-100 become 0-5 stars) and playlists become cases.</p>
    <Button onclick={() => interop.pick("itunes")} disabled={!lib || interop.loading}>Import…</Button>
  </section>

  <section class="card">
    <h3>Playlists and lists</h3>
    <p class="muted">Import an M3U / M3U8 playlist as a case (relative and absolute paths, #EXTINF titles), or save a case, a smart case, a mix or the whole library as an M3U8 playlist or a CSV list.</p>
    <div class="row">
      <Button onclick={() => interop.pick("m3u")} disabled={!lib || interop.loading}>Import playlist…</Button>
    </div>
    <div class="row export">
      <select bind:value={scope} disabled={!lib} aria-label="What to export">
        <option value="library">Whole library</option>
        {#if lib?.cases.length}
          <optgroup label="Cases">
            {#each lib.cases as c (c.id)}<option value={`case:${c.id}`}>{c.name}</option>{/each}
          </optgroup>
        {/if}
        {#if cases.smart.length}
          <optgroup label="Smart cases (as they match now)">
            {#each cases.smart as s (s.id)}<option value={`smart:${s.id}`}>{s.name}</option>{/each}
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
    <h3>Serato</h3>
    <p class="muted">Crates become cases. Pick the <code>_Serato_</code> folder (on your music drive, or in your Music folder) to read every crate in it, or a single <code>.crate</code> file. A crate holds file paths only, so tracks are matched by file name and size and the files must be reachable from this computer. Smart crates are not imported.</p>
    <div class="row">
      <Button onclick={() => interop.pick("serato-folder")} disabled={!lib || interop.loading}>Import _Serato_ folder…</Button>
      <Button onclick={() => interop.pick("serato-crate")} disabled={!lib || interop.loading}>Import .crate…</Button>
    </div>
  </section>

  <section class="card">
    <h3>Traktor</h3>
    <p class="muted">collection.nml: ratings, playlists and the first cue of each track become ratings, cases and (if you choose) cue points, previewed first. Colours and beat grids are not imported.</p>
    <Button onclick={() => interop.pick("traktor")} disabled={!lib || interop.loading}>Import collection.nml…</Button>
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
  .row.export {
    margin-top: 8px;
  }
  .row {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    align-items: center;
  }
  .opt {
    display: flex;
    gap: 6px;
    align-items: center;
    margin-top: 8px;
    font-size: 13px;
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
