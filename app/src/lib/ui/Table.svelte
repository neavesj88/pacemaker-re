<script module lang="ts">
  export type Column = { key: string; label: string; num?: boolean };
</script>

<script lang="ts" generics="T">
  import type { Snippet } from "svelte";

  // A sortable, striped table. The caller owns the rows and the sort: it passes already-sorted
  // rows plus sortKey / sortAsc for the header arrow, and handles onsort(key).
  let {
    columns,
    rows,
    rowKey,
    cell,
    sortKey,
    sortAsc = true,
    onsort,
    rowClass,
    onrowclick,
  }: {
    columns: Column[];
    rows: T[];
    rowKey: (row: T) => string | number;
    /** Renders one cell; without it the cell shows String(row[column.key]). */
    cell?: Snippet<[T, Column]>;
    sortKey?: string;
    sortAsc?: boolean;
    onsort?: (key: string) => void;
    rowClass?: (row: T) => string;
    onrowclick?: (row: T) => void;
  } = $props();
</script>

<table>
  <thead>
    <tr>
      {#each columns as c (c.key)}
        <th class:num={c.num} class:sortable={!!onsort} onclick={() => onsort?.(c.key)}>
          {c.label}{sortKey === c.key ? (sortAsc ? " ▲" : " ▼") : ""}
        </th>
      {/each}
    </tr>
  </thead>
  <tbody>
    {#each rows as row (rowKey(row))}
      <tr class={rowClass?.(row)} class:click={!!onrowclick} onclick={() => onrowclick?.(row)}>
        {#each columns as c (c.key)}
          <td class:num={c.num}>
            {#if cell}{@render cell(row, c)}{:else}{(row as Record<string, unknown>)[c.key]}{/if}
          </td>
        {/each}
      </tr>
    {/each}
  </tbody>
</table>

<style>
  table {
    width: 100%;
    border-collapse: collapse;
  }
  th {
    position: sticky;
    top: 0;
    background: var(--deep);
    text-align: left;
    font-weight: 600;
    color: var(--muted);
    padding: 6px 10px;
    border-bottom: 1px solid var(--line);
    user-select: none;
    white-space: nowrap;
  }
  th.sortable {
    cursor: pointer;
  }
  td {
    padding: 4px 10px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    max-width: 320px;
  }
  .num {
    text-align: right;
    font-variant-numeric: tabular-nums;
  }
  tbody tr:nth-child(odd) {
    background: var(--row);
  }
  tbody tr:nth-child(even) {
    background: var(--row-alt);
  }
  tbody tr:hover {
    background: var(--hover);
  }
  tbody :global(tr.dim) {
    color: var(--muted);
  }
  tr.click {
    cursor: pointer;
  }
</style>
