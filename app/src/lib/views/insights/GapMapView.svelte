<script lang="ts">
  // The Library gap map: how the collection spreads over BPM bands, keys, genres and (when the
  // tracks carry it) energy, with the thin spots called out. Everything is computed here, from
  // the tracks already loaded; nothing is read from or written to the device.
  import { energy } from "$lib/state/energy.svelte";
  import { filters } from "$lib/state/filters.svelte";
  import { nav } from "$lib/state/nav.svelte";
  import { session } from "$lib/state/session.svelte";
  import { toast } from "$lib/state/toast.svelte";
  import Button from "$lib/ui/Button.svelte";
  import { parseKey } from "../tracks/trackLogic";
  import { tracksState } from "../tracks/tracksState.svelte";
  import CellPanel from "./CellPanel.svelte";
  import { anyEnergy, BAND_WIDTHS, buildMatrix, cellFilter, MATRIX_IDS, summarise, thinCellSet, type GapSettings, type GapTrack, type KeyCode } from "./gapLogic";
  import { gap } from "./gapState.svelte";
  import { cellName, plural, TAB_LABEL } from "./gapText";
  import Heatmap from "./Heatmap.svelte";
  import ThinSummary from "./ThinSummary.svelte";

  const keyCode: KeyCode = (k) => parseKey(k)?.code ?? null;

  const library = $derived(session.library);
  // `energy` is not on the Track wire type: it comes from the auto energy estimate (app-side, see
  // state/energy.svelte.ts), so a track is placed on the energy axis only once it has been analysed
  const tracks = $derived((library?.tracks ?? []).map((t): GapTrack => ({ ...t, energy: energy.of(t.track_id) })));
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
  /** Applies one edited setting, then puts the field back to the value now in effect: when the
   *  setting was clamped back to what it already was, Svelte has nothing to rewrite and the typed
   *  number would otherwise stay on screen. */
  function change(e: Event, key: keyof GapSettings) {
    const el = e.currentTarget as HTMLInputElement | HTMLSelectElement;
    const next = gap.update({ [key]: Number(el.value) });
    el.value = String(next[key]);
  }
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
  <div class="settings">
    <label>
      BPM band
      <select value={gap.settings.bandWidth} onchange={(e) => change(e, "bandWidth")}>
        {#each BAND_WIDTHS as w (w)}<option value={w}>{w} BPM wide</option>{/each}
      </select>
    </label>
    <label>
      BPM range from
      <input type="number" min="20" max="300" step="1" value={gap.settings.bpmMin} onchange={(e) => change(e, "bpmMin")} />
    </label>
    <label>
      up to
      <input type="number" min="30" max="400" step="1" value={gap.settings.bpmMax} onchange={(e) => change(e, "bpmMax")} />
    </label>
    <label>
      Genres shown
      <input type="number" min="3" max="40" step="1" value={gap.settings.maxGenres} onchange={(e) => change(e, "maxGenres")} />
    </label>
    <label>
      Thin means fewer than
      <input type="number" min="1" max="20" step="1" value={gap.settings.thinBelow} onchange={(e) => change(e, "thinBelow")} />
      tracks
    </label>
    <label>
      Judge rows from
      <input type="number" min="1" max="500" step="1" value={gap.settings.minRowTracks} onchange={(e) => change(e, "minRowTracks")} />
      tracks
    </label>
    <Button small variant="ghost" onclick={() => gap.resetSettings()}>Reset to defaults</Button>
    <span class="muted note">Saved on this computer. The range is trimmed to a whole number of bands.</span>
    <span class="notice" role="status">{gap.notice}</span>
  </div>
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
        Key × Energy appears once tracks have an energy estimate.
        <button type="button" class="linkish" onclick={() => energy.openPanel()}>Analyse energy…</button>
      </span>
    {/if}
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
          {#if active === "key-energy"}No track has both a key and an energy rating yet.
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
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px 16px;
    padding: 8px 12px;
    background: var(--panel);
    border-bottom: 1px solid var(--line);
  }
  .settings label {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .settings input {
    width: 64px;
  }
  .note,
  .notice {
    font-size: 12px;
  }
  .notice {
    color: var(--cue);
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
