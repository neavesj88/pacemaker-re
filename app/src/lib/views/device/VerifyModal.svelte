<script lang="ts">
  import type { VerifyReport } from "$lib/api/device";
  import Button from "$lib/ui/Button.svelte";
  import { bytes } from "./format";
  import { STATUS_TEXT, statusCounts, verifyVerdict, visibleChecks, type Status } from "./trustLogic";
  import WideModal from "./WideModal.svelte";

  // The file-by-file result of a backup verification: every line of the manifest with what was
  // found, the problems first by default, and the files in the folder the manifest does not list.
  let { report, onclose }: { report: VerifyReport; onclose: () => void } = $props();

  const counts = $derived(statusCounts(report.files));
  const problems = $derived(report.files.length - counts.ok);
  // start on the problems if there are any; the user can show everything
  let problemsOnly = $state<boolean | null>(null);
  const only = $derived(problemsOnly ?? problems > 0);
  const rows = $derived(visibleChecks(report.files, only));
  const order: Status[] = ["differs", "missing", "unreadable", "unsafe", "bad_line"];
</script>

<WideModal title="Backup check" {onclose} width={760}>
  <p class="folder">{report.folder}</p>
  <p class:bad={problems > 0} class:good={problems === 0} role="status">
    <b>{verifyVerdict(report)}</b>
    <span class="muted"> {bytes(report.bytes)} read.</span>
  </p>
  {#if problems > 0}
    <p class="chips">
      {#each order as st (st)}
        {#if counts[st] > 0}<span class="chip">{counts[st]} {STATUS_TEXT[st].toLowerCase()}</span>{/if}
      {/each}
      <span class="chip ok">{counts.ok} match</span>
    </p>
  {/if}
  <label class="only">
    <input type="checkbox" checked={only} onchange={(e) => (problemsOnly = e.currentTarget.checked)} />
    Show only the files that do not match
  </label>
  {#if rows.length === 0}
    <p class="muted">Nothing to list here.</p>
  {:else}
    <table aria-label="Files in the backup">
      <thead><tr><th>File</th><th>Result</th><th class="r">Size</th></tr></thead>
      <tbody>
        {#each rows as f (f.path)}
          <tr class:badrow={f.status !== "ok"}>
            <td class="path">
              {f.path}
              {#if f.detail}<div class="detail">{f.detail}</div>{/if}
            </td>
            <td class="res" class:ok={f.status === "ok"}>{STATUS_TEXT[f.status]}</td>
            <td class="r num">{f.size === null ? "" : bytes(f.size)}</td>
          </tr>
        {/each}
      </tbody>
    </table>
  {/if}
  {#if report.extra.length}
    <details>
      <summary>{report.extra.length} {report.extra.length === 1 ? "file" : "files"} in the folder the manifest does not list</summary>
      <ul>
        {#each report.extra as x (x)}<li class="path">{x}</li>{/each}
      </ul>
    </details>
  {/if}
  {#snippet actions()}
    <Button data-autofocus variant="primary" onclick={onclose}>Close</Button>
  {/snippet}
</WideModal>

<style>
  .folder {
    margin: 0 0 6px;
    font-size: 12px;
    color: var(--muted);
    overflow-wrap: anywhere;
  }
  .good b {
    color: var(--ok);
  }
  .bad b {
    color: var(--bad);
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin: 0 0 8px;
  }
  .chip {
    font-size: 11px;
    padding: 1px 7px;
    border-radius: 9px;
    border: 1px solid var(--bad);
    color: var(--bad-text);
  }
  .chip.ok {
    border-color: var(--line-hi);
    color: var(--ok);
  }
  .only {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 4px 0 8px;
  }
  table {
    width: 100%;
    border-collapse: collapse;
    font-size: 12px;
  }
  th {
    text-align: left;
    color: var(--muted);
    font-weight: 500;
    padding: 3px 6px;
    border-bottom: 1px solid var(--line);
    position: sticky;
    top: 0;
    background: var(--panel);
  }
  td {
    padding: 3px 6px;
    vertical-align: top;
  }
  tbody tr:nth-child(odd) {
    background: var(--row);
  }
  .badrow {
    background: var(--bad-bg) !important;
  }
  .path {
    font-family: ui-monospace, Consolas, monospace;
    overflow-wrap: anywhere;
  }
  .detail {
    font-family: inherit;
    color: var(--muted);
    font-size: 11px;
  }
  .res {
    white-space: nowrap;
    color: var(--bad-text);
  }
  .res.ok {
    color: var(--ok);
  }
  .r {
    text-align: right;
    white-space: nowrap;
  }
  details {
    margin-top: 10px;
  }
  details ul {
    margin: 6px 0 0;
    padding-left: 18px;
    max-height: 120px;
    overflow: auto;
  }
</style>
