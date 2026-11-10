<script lang="ts">
  import { volumeName } from "$lib/format";
  import { session } from "$lib/state/session.svelte";
  import Button from "$lib/ui/Button.svelte";
  import RMark from "$lib/ui/RMark.svelte";
</script>

<div class="welcome">
  <RMark size={96} />
  <h1>Connect your Pacemaker</h1>
  {#if session.volumes.length > 0}
    <p>Found on this computer:</p>
    <div class="actions">
      {#each session.volumes as v}
        <Button variant="primary" onclick={() => session.load(v)}>Open {volumeName(v)}</Button>
      {/each}
    </div>
  {:else}
    <p>Dock it over USB: it appears as a drive, then press Rescan.</p>
  {/if}
  <p>Or open a folder that contains a <code>.Pacemaker</code> library.</p>
  <div class="actions">
    <Button onclick={() => session.refreshVolumes()}>Rescan</Button>
    <Button onclick={() => session.openFolder()}>Open folder…</Button>
  </div>
</div>

<style>
  .welcome {
    margin: auto;
    text-align: center;
    color: var(--muted);
  }
  h1 {
    color: var(--text);
    font-weight: 600;
  }
  .actions {
    display: flex;
    gap: 8px;
    justify-content: center;
    margin-top: 16px;
  }
</style>
