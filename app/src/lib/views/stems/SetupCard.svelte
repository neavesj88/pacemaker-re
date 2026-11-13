<script lang="ts">
  import type { Component } from "$lib/api/stems";
  import { stems } from "$lib/state/stems.svelte";
  import Button from "$lib/ui/Button.svelte";
  import Modal from "$lib/ui/Modal.svelte";
  import LicenceModal from "./LicenceModal.svelte";
  import { componentState, formatBytes } from "./logic";

  // The model and the engine that runs it are not shipped with the app: each is downloaded from
  // its pinned address after the user has read its licence notice.
  let asking = $state<Component | null>(null);
  let removing = $state<Component | null>(null);

  const st = $derived(stems.status);
  const dl = $derived(stems.download);
  const pct = $derived(dl && dl.total > 0 ? Math.round((dl.done / dl.total) * 100) : 0);

  async function confirmRemove() {
    const c = removing;
    removing = null;
    if (c) await stems.remove(c.id);
  }
</script>

<section class="card">
  <header>
    <h4>Model and engine</h4>
    {#if st}<span class="state" class:ok={st.ready}>{st.ready ? "Ready" : "Not ready"}</span>{/if}
  </header>

  {#if !st}
    <p class="muted">{stems.statusError ? `Could not read the status: ${stems.statusError}` : "Reading the status…"}</p>
  {:else}
    <p class="muted intro">
      Separation runs on this computer; no audio leaves it. The model and the engine are downloaded once, only after you accept each notice.
    </p>
    {#each st.components as c (c.id)}
      {@const s = componentState(c)}
      <div class="part">
        <div class="top">
          <strong>{c.name}</strong>
          {#if c.licence_status === "unclear"}<span class="badge unclear">licence unclear</span>{/if}
        </div>
        <div class="muted">{c.summary}</div>
        <div class="row">
          <span class="s {s.tone}">{s.text}</span>
          <span class="sp"></span>
          {#if c.installed}
            <Button small disabled={stems.busy} onclick={() => (removing = c)}>Remove</Button>
          {:else if c.available_here}
            <Button small variant="primary" disabled={stems.busy} onclick={() => (asking = c)}>Review notice and download</Button>
          {/if}
        </div>
        {#if dl?.id === c.id}
          <div class="bar"><i style="width:{pct}%"></i></div>
          <div class="line muted">
            <span>{dl.detail || "Downloading…"}</span>
            <Button small variant="ghost" disabled={dl.cancelling} onclick={() => stems.cancelDownload()}>{dl.cancelling ? "Stopping…" : "Cancel"}</Button>
          </div>
        {/if}
      </div>
    {/each}

    <div class="part">
      <div class="top"><strong>Engine in use</strong></div>
      {#if st.runtime.path}
        <div class="row"><code class="path">{st.runtime.path}</code></div>
        <div class="muted">{st.runtime.source === "custom" ? "Chosen in Settings below." : "Downloaded above."}</div>
      {:else}
        <div class="muted">{st.runtime.problem}</div>
      {/if}
    </div>

    {#if st.blocked && !dl}<p class="why" role="status">{st.blocked}</p>{/if}
  {/if}
  {#if stems.statusError && st}<p class="why" role="alert">{stems.statusError}</p>{/if}
</section>

{#if asking}
  <LicenceModal part={asking} onclose={() => (asking = null)} />
{/if}

{#if removing}
  <Modal title={`Remove ${removing.name}?`} onclose={() => (removing = null)}>
    <p>This deletes the downloaded file ({formatBytes(removing.installed_bytes)}) from this app's data folder:</p>
    <p><code>{st?.data_dir}</code></p>
    <p class="muted">Your music and the Pacemaker are not touched. You can download it again later.</p>
    {#snippet actions()}
      <Button data-autofocus onclick={() => (removing = null)}>Keep it</Button>
      <Button variant="primary" onclick={confirmRemove}>Remove</Button>
    {/snippet}
  </Modal>
{/if}

<style>
  .card {
    background: var(--panel);
    border: 1px solid var(--line);
    border-radius: 8px;
    padding: 12px;
    min-width: 0;
  }
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 8px;
  }
  h4 {
    margin: 0;
    font-size: 13px;
  }
  .state {
    font-size: 11px;
    padding: 1px 8px;
    border-radius: 9px;
    background: var(--accent-soft);
    color: var(--accent);
  }
  .state.ok {
    background: color-mix(in srgb, var(--green) 10%, transparent);
    color: var(--green);
  }
  .intro {
    margin: 0 0 8px;
    font-size: 12px;
  }
  .part {
    border-top: 1px solid var(--line);
    padding: 8px 0;
  }
  .top {
    display: flex;
    gap: 8px;
    align-items: center;
    flex-wrap: wrap;
  }
  .badge {
    font-size: 11px;
    padding: 0 7px;
    border-radius: 9px;
  }
  .badge.unclear {
    background: var(--bad-bg);
    color: var(--bad-text);
    border: 1px solid var(--bad);
  }
  .row {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-top: 4px;
  }
  .sp {
    flex: 1;
  }
  .s.ok {
    color: var(--ok);
  }
  .s.todo {
    color: var(--accent);
  }
  .s.na {
    color: var(--muted);
  }
  .path {
    overflow-wrap: anywhere;
    font-size: 12px;
  }
  .bar {
    height: 8px;
    border-radius: 4px;
    background: var(--row-alt);
    overflow: hidden;
    margin-top: 6px;
  }
  .bar i {
    display: block;
    height: 100%;
    background: var(--accent-ramp);
    transition: width 0.2s;
  }
  .line {
    display: flex;
    justify-content: space-between;
    align-items: center;
    font-size: 12px;
    margin-top: 2px;
  }
  .why {
    margin: 8px 0 0;
    padding: 6px 10px;
    border-radius: 4px;
    background: var(--bad-bg);
    color: var(--bad-text);
    font-size: 12px;
  }
</style>
