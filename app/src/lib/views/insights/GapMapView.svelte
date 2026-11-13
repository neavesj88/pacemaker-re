<script lang="ts">
  // The Library gap map: how the collection spreads over BPM bands, keys, genres and (when the
  // tracks carry it) energy, with the thin spots called out. Everything is computed here, from
  // the tracks already loaded; nothing is read from or written to the device.
  import { energy } from "$lib/state/energy.svelte";
  import { energySource } from "$lib/state/energySource.svelte";
  import EnergySourceSelect from "../metadata/EnergySourceSelect.svelte";
  import { filters } from "$lib/state/filters.svelte";
  import { nav } from "$lib/state/nav.svelte";
  import { session } from "$lib/state/session.svelte";
  import { toast } from "$lib/state/toast.svelte";
  import Button from "$lib/ui/Button.svelte";
  import { parseKey } from "../tracks/trackLogic";
  import { tracksState } from "../tracks/tracksState.svelte";
  import CellPanel from "./CellPanel.svelte";
  import { anyEnergy, buildMatrix, cellFilter, MATRIX_IDS, summarise, thinCellSet, type GapTrack, type KeyCode } from "./gapLogic";
  import GapSettingsFields from "./GapSettingsFields.svelte";
  import { gap } from "./gapState.svelte";
  import { cellName, plural, TAB_LABEL } from "./gapText";
  import Heatmap from "./Heatmap.svelte";
  import ThinSummary from "./ThinSummary.svelte";

  const keyCode: KeyCode = (k) => parseKey(k)?.code ?? null;

  const library = $derived(session.library);
  // `energy` is not on the Track wire type: it is the energy under the "Energy source" setting
  // (your rating, the audio estimate, or yours else the estimate; state/energySource.svelte.ts), so a
  // track is placed on the energy axis only once it has one
  const tracks = $derived((library?.tracks ?? []).map((t): GapTrack => ({ ...t, energy: energySource.level(t.track_id) })));
  const hasEnergy = $derived(anyEnergy(tracks));
  const tabs = $derived(MATRIX_IDS.filter((id) => id !== "key-energy" || hasEnergy));
  const active = $derived(tabs.includes(gap.matrix) ? gap.matrix : tabs[0]);

  const matrix = $derived(buildMatrix(active, tracks, session.needs, gap.settings, keyCode));
  const summary = $derived(summarise(matrix, gap.settings));
  const thin = $derived(thinCellSet(summary));

  // the picked cell as indices into the current grid (null if its row or column is gone)
  const picked = $derived.by(() => {
    const p = gap.picked;
    if (!p) return null;
    const row = matrix.rows.findIndex((b) => b.id === p.row);
    const col = matrix.cols.findIndex((b) => b.id === p.col);
    return row >= 0 && col >= 0 ? { row, col } : null;
  });
  const byId = $derived(new Map(tracks.map((t) => [t.track_id, t])));
  const cellTracks = $derived(picked ? matrix.cells[picked.row][picked.col].flatMap((id) => byId.get(id) ?? []) : []);
  const filter = $derived(picked ? cellFilter(matrix, picked.row, picked.col, tracks, session.needs) : null);

  const whyNot = $derived.by(() => {
    if (!picked) return "";
    const kinds = [matrix.rows[picked.row].value.kind, matrix.cols[picked.col].value.kind];
    if (kinds.includes("energy")) return "The Tracks view has no energy filter.";
    if (kinds.includes("otherGenres")) return "These span several genres, which the Tracks view filters one at a time.";
    return "";
  });

  function pick(row: number, col: number) {
    gap.picked = { row: matrix.rows[row].id, col: matrix.cols[col].id };
  }

  /** Sets the Tracks view's own filters to the cell's, so what it lists is what this cell counted. */
  function openInTracks() {
    if (!picked || !filter) return;
    tracksState.clearFilters();
    filters.query = "";
    filters.onlyNeeds = false;
    tracksState.genre = filter.genre;
    tracksState.bpmMin = filter.bpmMin === null ? "" : String(filter.bpmMin);
    tracksState.bpmMax = filter.bpmMax === null ? "" : String(filter.bpmMax);
    tracksState.keyFilter = filter.key;
    nav.go("tracks");
    toast.show(`Tracks now shows ${cellName(matrix, picked.row, picked.col)}. Use Clear filters there to see everything again.`, "info", 6000);
  }

  function selectTab(id: (typeof tabs)[number]) {
    gap.matrix = id;
    gap.picked = null;
  }

  let settingsOpen = $state(false);
  const skippedTotal = $derived(matrix.skipped.reduce((n, s) => n + s.count, 0));
</script>

<div class="gap">
<div class="bar">
  <h2>Library gap map</h2>
  <span class="muted">Where the collection is thick and where it is thin. Worked out here from the tracks on the card; nothing is written.</span>
  <span class="grow"></span>
  <Button small active={settingsOpen} aria-expanded={settingsOpen} onclick={() => (settingsOpen = !settingsOpen)}>Settings</Button>
</div>

{#if settingsOpen}
  <div class="settings"><GapSettingsFields /></div>
{/if}

{#if !library}
  <p class="empty">Open a library to see its gap map.</p>
{:else if tracks.length === 0}
  <p class="empty">This library has no tracks yet, so there is nothing to map.</p>
{:else}
  <div class="tabs" role="tablist" aria-label="Gap map grids">
    {#each tabs as id (id)}
      <button id="gap-tab-{id}" role="tab" aria-selected={active === id} aria-controls="gap-panel" class:on={active === id} onclick={() => selectTab(id)}>
        {TAB_LABEL[id]}
      </button>
    {/each}
    {#if !hasEnergy}
      <span class="hint muted">
        Key × Energy appears once tracks have an energy rating or an estimate.
        <button type="button" class="linkish" onclick={() => energy.openPanel()}>Analyse energy…</button>
      </span>
    {/if}
    <span class="esrc"><EnergySourceSelect compact /></span>
  </div>

  <div class="body" id="gap-panel" role="tabpanel" aria-labelledby="gap-tab-{active}">
    <div class="main">
      <div class="stats" role="status">
        <b>{matrix.total}</b> of {plural(matrix.considered, "track")} placed
        {#if skippedTotal > 0}
          <span class="muted">· left out:</span>
          {#each matrix.skipped as s (s.reason)}<span class="out">{s.count} with {s.reason}</span>{/each}
        {/if}
        <span class="grow"></span>
        <span class="legend" aria-hidden="true"><i class="sw lo"></i>few<i class="sw hi"></i>many<i class="sw thinsw"></i>thin</span>
      </div>
      {#if matrix.total === 0}
        <p class="empty">
          {#if active === "key-energy"}No track has both a key and an energy (a rating or an estimate, under the Energy source setting) yet.
          {:else}No track has a BPM, {active === "bpm-key" ? "a key" : "a genre"} and a BPM inside {gap.settings.bpmMin}–{gap.settings.bpmMax}. Tracks that still need analysis have no BPM here.
          {/if}
        </p>
      {:else}
        <Heatmap {matrix} {thin} {picked} onpick={pick} />
      {/if}
    </div>

    <aside class="side">
      {#if picked}
        <CellPanel
          title={cellName(matrix, picked.row, picked.col)}
          tracks={cellTracks}
          canOpen={!!filter}
          {whyNot}
          showEnergy={hasEnergy}
          onopen={openInTracks}
          onclose={() => (gap.picked = null)}
        />
      {/if}
      <ThinSummary {matrix} {summary} settings={gap.settings} onpick={pick} />
    </aside>
  </div>
{/if}
</div>

<style>
  .gap {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
  }
  .bar {
    flex: none;
    display: flex;
    align-items: baseline;
    gap: 12px;
    padding: 10px 12px;
    border-bottom: 1px solid var(--line);
  }
  h2 {
    margin: 0;
    font-size: 16px;
  }
  .grow {
    flex: 1;
  }
  .settings {
    padding: 8px 12px;
    background: var(--panel);
    border-bottom: 1px solid var(--line);
  }
  .tabs {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 2px;
    padding: 0 12px;
    border-bottom: 1px solid var(--line);
  }
  .tabs button {
    background: none;
    border: none;
    border-bottom: 2px solid transparent;
    color: var(--muted);
    padding: 8px 12px;
    font: inherit;
    cursor: pointer;
  }
  .tabs button:hover {
    color: var(--text);
  }
  .tabs button.on {
    color: var(--text);
    border-bottom-color: var(--accent);
  }
  .esrc {
    margin-left: auto;
  }
  .hint {
    margin-left: 12px;
    font-size: 12px;
  }
  .linkish {
    background: none;
    border: none;
    padding: 0;
    font: inherit;
    color: var(--accent);
    cursor: pointer;
    text-decoration: underline;
  }
  .body {
    display: flex;
    min-height: 0;
    flex: 1;
    overflow: hidden;
  }
  .main {
    flex: 1;
    min-width: 0;
    overflow: auto;
  }
  .side {
    flex: none;
    width: 360px;
    overflow: auto;
    border-left: 1px solid var(--line);
    background: var(--bg);
  }
  .stats {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 4px 10px;
    padding: 8px 12px 0;
  }
  .out {
    border: 1px solid var(--line-hi);
    border-radius: 10px;
    padding: 0 8px;
    font-size: 12px;
    color: var(--muted);
  }
  .legend {
    display: flex;
    align-items: center;
    gap: 5px;
    font-size: 11px;
    color: var(--muted);
  }
  .sw {
    display: inline-block;
    width: 14px;
    height: 10px;
    border: 1px solid var(--line);
    border-radius: 2px;
  }
  .sw.lo {
    background: color-mix(in srgb, var(--accent) 14%, var(--panel));
  }
  .sw.hi {
    background: color-mix(in srgb, var(--accent) 70%, var(--panel));
    margin-left: 6px;
  }
  .sw.thinsw {
    border: 1px dashed var(--muted);
    margin-left: 6px;
  }
  @media (max-width: 900px) {
    .body {
      flex-direction: column;
      overflow: auto;
    }
    .side {
      width: auto;
      border-left: none;
      border-top: 1px solid var(--line);
      overflow: visible;
    }
  }
</style>
