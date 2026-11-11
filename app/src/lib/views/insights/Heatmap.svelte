<script lang="ts">
  // One grid of the gap map: a header row, one row per bin, a count in every cell. A cell is a
  // button; the picked one is outlined, thin cells (see gapLogic.summarise) have a dashed edge.
  import type { Matrix } from "./gapLogic";
  import { AXES, binHint, binName, cellName, plural } from "./gapText";

  let {
    matrix,
    thin,
    picked,
    onpick,
  }: {
    matrix: Matrix;
    /** "row:col" of the cells to mark as thin. */
    thin: Set<string>;
    picked: { row: number; col: number } | null;
    onpick: (row: number, col: number) => void;
  } = $props();

  const axes = $derived(AXES[matrix.id]);
  // a wider first column when the rows carry genre names
  const wideHead = $derived(matrix.rows.some((r) => r.value.kind === "genre" || r.value.kind === "otherGenres"));
  // 24 keys are two groups of 12 on the Camelot wheel (A, then B): leave a gap between them
  const splitAt = $derived(matrix.cols.length === 24 && matrix.cols[0].value.kind === "key" ? 12 : -1);

  /** Share of the cell colour: square-root scale so a few busy cells do not wash the rest out. */
  const heat = (count: number) => (count === 0 || matrix.max === 0 ? 0 : Math.round(12 + 58 * Math.sqrt(count / matrix.max)));
</script>

<div class="wrap">
  <div class="grid" class:wide={wideHead} style:--cols={matrix.cols.length} role="group" aria-label="{axes.row} by {axes.col}">
    <div class="corner">{axes.row} ↓ · {axes.col} →</div>
    {#each matrix.cols as c, ci (c.id)}
      <div class="colh" class:split={ci === splitAt} title={binHint(c)}>{c.label}<small>{matrix.colTotals[ci]}</small></div>
    {/each}

    {#each matrix.rows as r, ri (r.id)}
      <div class="rowh" title={binHint(r)}>
        <span class="lab">{r.label}</span><small>{matrix.rowTotals[ri]}</small>
      </div>
      {#each matrix.cols as c, ci (c.id)}
        {@const n = matrix.cells[ri][ci].length}
        {@const isThin = thin.has(`${ri}:${ci}`)}
        <button
          class="cell"
          class:split={ci === splitAt}
          class:thin={isThin}
          class:on={picked?.row === ri && picked?.col === ci}
          style:--heat="{heat(n)}%"
          aria-pressed={picked?.row === ri && picked?.col === ci}
          aria-label="{cellName(matrix, ri, ci)}: {plural(n, 'track')}{isThin ? ', thin' : ''}"
          title="{cellName(matrix, ri, ci)}: {plural(n, 'track')}{isThin ? ' (thin)' : ''}"
          onclick={() => onpick(ri, ci)}
        >
          {#if n > 0}{n}{:else if isThin}0{:else}<span class="dot">·</span>{/if}
        </button>
      {/each}
    {/each}
  </div>
</div>

<style>
  .wrap {
    overflow: auto;
    padding: 12px;
  }
  .grid {
    --head: 84px;
    display: grid;
    grid-template-columns: var(--head) repeat(var(--cols), minmax(34px, 1fr));
    gap: 2px;
    width: max-content;
    min-width: 100%;
  }
  .grid.wide {
    --head: 150px;
  }
  .corner {
    position: sticky;
    left: 0;
    z-index: 1;
    background: var(--bg);
    font-size: 11px;
    color: var(--muted);
    align-self: end;
    padding: 0 6px 4px 0;
    line-height: 1.2;
  }
  .colh,
  .rowh {
    font-size: 12px;
    color: var(--muted);
    font-variant-numeric: tabular-nums;
  }
  .colh {
    text-align: center;
    padding-bottom: 4px;
    display: flex;
    flex-direction: column;
    align-items: center;
  }
  .rowh {
    position: sticky;
    left: 0;
    z-index: 1;
    background: var(--bg);
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 6px;
    padding-right: 6px;
    color: var(--text);
  }
  .lab {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  small {
    color: var(--muted);
    font-size: 10px;
  }
  .split {
    margin-left: 10px;
  }
  .cell {
    height: 30px;
    border: 1px solid var(--line);
    border-radius: 3px;
    background: color-mix(in srgb, var(--accent) var(--heat), var(--panel));
    color: var(--text);
    font: inherit;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
    cursor: pointer;
    padding: 0;
  }
  .cell:hover {
    border-color: var(--text);
  }
  .cell.thin {
    border: 1px dashed var(--muted);
  }
  .cell.on {
    outline: 2px solid var(--green);
    outline-offset: 1px;
    border-color: var(--green);
  }
  .cell:focus-visible {
    outline: 2px solid var(--text);
    outline-offset: 1px;
  }
  .dot {
    color: var(--line-hi);
    font-weight: 400;
  }
  .cell.thin:not(.on) {
    color: var(--muted);
  }
</style>
