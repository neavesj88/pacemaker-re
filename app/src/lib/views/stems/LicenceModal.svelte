<script lang="ts">
  import type { Component } from "$lib/api/stems";
  import { stems } from "$lib/state/stems.svelte";
  import Button from "$lib/ui/Button.svelte";
  import Modal from "$lib/ui/Modal.svelte";
  import { formatBytes } from "./logic";

  // The notice shown before a part is downloaded. Nothing is fetched until the button here is
  // pressed, and the backend refuses a download whose notice version differs from the one shown.
  let { part, onclose }: { part: Component; onclose: () => void } = $props();

  let read = $state(false);
  const unclear = $derived(part.licence_status === "unclear");
  let agreed = $state(false);
  const ok = $derived(read && (!unclear || agreed));

  async function go() {
    // read before closing: the parent clears `part` when the dialog closes
    const { id, notice_version } = part;
    onclose();
    await stems.startDownload(id, notice_version);
  }
</script>

<Modal title={`Download ${part.name}?`} {onclose}>
  <div class="head">
    <span class="badge" class:unclear>{unclear ? "Licence unclear" : "Licence clear"}</span>
    <span class="muted">{part.licence_name}</span>
  </div>

  <p class="notice">{part.licence_text}</p>

  <dl>
    <dt>From</dt>
    <dd><code>{part.url}</code></dd>
    <dt>More information</dt>
    <dd><code>{part.page_url}</code></dd>
    <dt>Size</dt>
    <dd>{formatBytes(part.size_bytes)} ({part.size_bytes.toLocaleString()} bytes)</dd>
    <dt>SHA-256</dt>
    <dd><code class="hash">{part.sha256}</code></dd>
    <dt>Saved to</dt>
    <dd>this app's own data folder ({stems.status?.data_dir ?? ""}); nothing is written to the Pacemaker</dd>
  </dl>
  <p class="muted small">The file is checked against the SHA-256 above while it downloads and is discarded if it does not match. You can remove it again here at any time.</p>

  <label class="tick"><input type="checkbox" bind:checked={read} data-autofocus /> I have read this notice and want to download this file.</label>
  {#if unclear}
    <label class="tick"><input type="checkbox" bind:checked={agreed} /> I understand the licence of these weights is unclear and I use the results at my own risk.</label>
  {/if}

  {#snippet actions()}
    <Button onclick={onclose}>Cancel</Button>
    <Button variant="primary" disabled={!ok} onclick={go}>Download ({formatBytes(part.size_bytes)})</Button>
  {/snippet}
</Modal>

<style>
  .head {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-bottom: 6px;
  }
  .badge {
    font-size: 11px;
    padding: 1px 8px;
    border-radius: 9px;
    background: color-mix(in srgb, var(--green) 10%, transparent);
    color: var(--green);
  }
  .badge.unclear {
    background: var(--bad-bg);
    color: var(--bad-text);
    border: 1px solid var(--bad);
  }
  .notice {
    white-space: pre-wrap;
    max-height: 260px;
    overflow: auto;
    margin: 6px 0;
    padding: 8px 10px;
    background: var(--deep);
    border: 1px solid var(--line);
    border-radius: 4px;
    font-size: 12px;
  }
  dl {
    display: grid;
    grid-template-columns: max-content 1fr;
    gap: 3px 12px;
    margin: 8px 0;
    font-size: 12px;
  }
  dt {
    color: var(--muted);
  }
  dd {
    margin: 0;
    min-width: 0;
    overflow-wrap: anywhere;
  }
  .hash {
    font-size: 11px;
  }
  .small {
    font-size: 12px;
    margin: 4px 0 8px;
  }
  .tick {
    display: flex;
    gap: 8px;
    align-items: flex-start;
    margin: 6px 0;
  }
  .tick input {
    margin-top: 2px;
  }
</style>
