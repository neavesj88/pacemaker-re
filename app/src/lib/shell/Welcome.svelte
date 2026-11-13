<script lang="ts">
  import { volumeName } from "$lib/format";
  import { session } from "$lib/state/session.svelte";
  import { welcomeOffer } from "$lib/state/place";
  import Button from "$lib/ui/Button.svelte";
  import RMark from "$lib/ui/RMark.svelte";

  const offer = $derived(welcomeOffer(session.computer, session.volumes));
</script>

<div class="welcome">
  <RMark size={96} />
  {#if offer.start}
    <h1>Start your library</h1>
    <p>Your tracks, mixes and cases live in a library on this computer. You do not need the Pacemaker to work on them.</p>
    <div class="actions">
      <Button variant="primary" onclick={() => session.startComputer()}>Start a library on this computer</Button>
      {#if offer.copyFrom}
        <Button onclick={() => session.copyFromPacemaker(offer.copyFrom!)} title="Makes the library on this computer, then shows what would be copied from the Pacemaker before anything is">
          Copy my Pacemaker's library to this computer
        </Button>
      {/if}
    </div>
    <p class="where">
      It will be made in <code>{session.computer?.path}</code>
      <Button small variant="link" onclick={() => session.chooseComputerFolder()}>Change…</Button>
    </p>
  {:else if offer.open}
    <h1>Open your library</h1>
    <div class="actions">
      <Button variant="primary" onclick={() => session.computer && session.load(session.computer.path)}>Open the library on this computer</Button>
    </div>
    <p class="where">It is in <code>{session.computer?.path}</code></p>
  {:else}
    <h1>Connect your Pacemaker</h1>
  {/if}

  <h2>{offer.start || offer.open ? "Or work on a Pacemaker directly" : "Open a Pacemaker"}</h2>
  {#if session.volumes.length > 0}
    <p>Pacemakers found:</p>
    <div class="actions">
      {#each session.volumes as v}
        <Button variant={offer.start || offer.open ? "default" : "primary"} onclick={() => session.load(v)}>Open {volumeName(v)}</Button>
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
    max-width: 640px;
    padding: 0 16px;
  }
  h1 {
    color: var(--text);
    font-weight: 600;
  }
  h2 {
    color: var(--text);
    font-weight: 600;
    font-size: 14px;
    margin: 28px 0 4px;
  }
  .actions {
    display: flex;
    gap: 8px;
    justify-content: center;
    flex-wrap: wrap;
    margin-top: 16px;
  }
  .where {
    font-size: 12px;
    overflow-wrap: anywhere;
  }
</style>
