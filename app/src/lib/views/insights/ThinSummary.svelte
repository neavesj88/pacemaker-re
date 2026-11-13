<script lang="ts">
  // Where the library is thin, in words. Every cell, row and column named here is a button that
  // picks the cell (or, for a whole row or column, its first cell) so the tracks can be looked at.
  import type { GapSettings, Matrix, Summary } from "./gapLogic";
  import { AXES, binName, plural } from "./gapText";

  let {
    matrix,
    summary,
    settings,
    onpick,
  }: {
    matrix: Matrix;
    summary: Summary;
    settings: GapSettings;
    onpick: (row: number, col: number) => void;
  } = $props();

  const axes = $derived(AXES[matrix.id]);
  // long lists stay readable: the first few, then "and N more"
  const SHOW = 14;
  let expanded = $state<Record<string, boolean>>({});
  const cap = <T,>(key: string, list: T[]): T[] => (expanded[key] ? list : list.slice(0, SHOW));
  const more = (key: string, n: number) => (!expanded[key] && n > SHOW ? n - SHOW : 0);

  const tooFew = $derived(settings.thinBelow === 2 ? "one track" : `up to ${settings.thinBelow - 1} tracks`);
  const nothing = $derived(
    summary.thinRows.length === 0 && summary.emptyRows.length === 0 && summary.emptyCols.length === 0 && summary.thinCols.length === 0,
  );
</script>

<section class="sum" aria-label="Thin areas">
  <h3>Thin areas</h3>
  <p class="muted rule">
    A cell is thin when it holds fewer than {plural(settings.thinBelow, "track")}. Rows with fewer than {plural(settings.minRowTracks, "track")} are
    not judged. Thin cells have a dashed edge on the grid.
  </p>

  {#if matrix.total === 0}
    <p class="muted">Nothing to judge yet: no track has all the values this grid needs.</p>
  {:else if nothing}
    <p class="good">No thin areas at these settings.</p>
  {/if}

  {#if summary.emptyRows.length > 0}
    <h4>No tracks at all</h4>
    <p class="muted">Between the lowest and highest {axes.row.toLowerCase()} in use:</p>
    <div class="chips">
      {#each summary.emptyRows as r (r)}
        <button class="chip" onclick={() => onpick(r, 0)} title="Pick the first cell of this row">{binName(matrix.rows[r])}</button>
      {/each}
    </div>
  {/if}
  {#if summary.emptyCols.length > 0}
    <h4>No tracks at all</h4>
    <p class="muted">Between the lowest and highest {axes.col.toLowerCase()} in use:</p>
    <div class="chips">
      {#each summary.emptyCols as c (c)}
        <button class="chip" onclick={() => onpick(0, c)} title="Pick the first cell of this column">{binName(matrix.cols[c])}</button>
      {/each}
    </div>
  {/if}

  {#each summary.thinRows as t (t.row)}
    <div class="row">
      <h4>{binName(matrix.rows[t.row])} <span class="muted">{plural(t.total, "track")}</span></h4>
      {#if t.empty.length > 0}
        <p class="muted lab">No tracks in {plural(t.empty.length, axes.col.toLowerCase())}:</p>
        <div class="chips">
          {#each cap(`e${t.row}`, t.empty) as c (c)}
            <button class="chip" onclick={() => onpick(t.row, c)}>{matrix.cols[c].label}</button>
          {/each}
          {#if more(`e${t.row}`, t.empty.length)}
            <button class="chip more" onclick={() => (expanded[`e${t.row}`] = true)}>+{more(`e${t.row}`, t.empty.length)} more</button>
          {/if}
        </div>
      {/if}
      {#if t.few.length > 0}
        <p class="muted lab">Only {tooFew}:</p>
        <div class="chips">
          {#each cap(`f${t.row}`, t.few) as f (f.col)}
            <button class="chip" onclick={() => onpick(t.row, f.col)}>{matrix.cols[f.col].label} <b>{f.count}</b></button>
          {/each}
          {#if more(`f${t.row}`, t.few.length)}
            <button class="chip more" onclick={() => (expanded[`f${t.row}`] = true)}>+{more(`f${t.row}`, t.few.length)} more</button>
          {/if}
        </div>
      {/if}
    </div>
  {/each}

  {#if summary.thinCols.length > 0}
    <h4>Thin across the whole grid</h4>
    <p class="muted">{axes.col} values with fewer than {plural(settings.thinBelow, "track")} in total:</p>
    <div class="chips">
      {#each cap("tc", summary.thinCols) as t (t.col)}
        <button class="chip" onclick={() => onpick(0, t.col)} title="Pick the first cell of this column">
          {matrix.cols[t.col].label} <b>{t.total}</b>
        </button>
      {/each}
      {#if more("tc", summary.thinCols.length)}
        <button class="chip more" onclick={() => (expanded["tc"] = true)}>+{more("tc", summary.thinCols.length)} more</button>
      {/if}
    </div>
  {/if}

  {#if summary.busiest}
    <p class="muted busy">
      Busiest cell: {binName(matrix.rows[summary.busiest.row])} × {binName(matrix.cols[summary.busiest.col])},
      {plural(summary.busiest.count, "track")}.
    </p>
  {/if}
</section>

<style>
  .sum {
    padding: 10px 12px 14px;
  }
  h3 {
    margin: 0 0 4px;
    font-size: 14px;
  }
  h4 {
    margin: 12px 0 2px;
    font-size: 13px;
  }
  p {
    margin: 2px 0 4px;
  }
  .rule {
    font-size: 12px;
  }
  .lab {
    margin: 4px 0 3px;
    font-size: 12px;
  }
  .good {
    color: var(--ok);
  }
  .row {
    border-top: 1px solid var(--line);
    margin-top: 10px;
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
  }
  .chip {
    background: var(--panel);
    color: var(--text);
    border: 1px dashed var(--muted);
    border-radius: 10px;
    padding: 1px 8px;
    font: inherit;
    font-size: 12px;
    font-variant-numeric: tabular-nums;
    cursor: pointer;
  }
  .chip:hover {
    border-color: var(--text);
    background: var(--hover);
  }
  .chip b {
    color: var(--accent);
    margin-left: 3px;
  }
  .chip.more {
    border-style: solid;
    color: var(--muted);
  }
  .busy {
    margin-top: 14px;
    font-size: 12px;
  }
</style>
