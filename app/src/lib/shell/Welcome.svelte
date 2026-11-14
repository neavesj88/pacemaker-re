<script lang="ts">
  import { volumeName } from "$lib/format";
  import { session } from "$lib/state/session.svelte";
  import { welcomeOffer } from "$lib/state/place";
  import Button from "$lib/ui/Button.svelte";
  import RMark from "$lib/ui/RMark.svelte";

  const offer = $derived(welcomeOffer(session.computer, session.volumes));

  // a library on a drive that was not connected a moment ago may be there now
  const rescan = () => Promise.all([session.refreshVolumes(), session.refreshComputer()]);
</script>

<div class="welcome">
  <RMark size={96} />
  {#if offer.unreadable}
    <h1>Your library settings could not be read</h1>
    <p>
      Nothing was opened, because the file that says where your library is cannot be trusted. The message above names the file.
      Open a folder or a Pacemaker below, or save the settings again to use the default location. A copy of the damaged file is
      kept next to it, with <code>.bad</code> added to its name.
    </p>
    <div class="actions">
      <Button variant="primary" onclick={() => session.saveComputer({ path: "", open_at_start: true })} title="Replaces the damaged settings with the defaults (the Music folder, in a folder called Pacemaker Revival). Nothing is opened or made yet.">
        Use the default location
      </Button>
      <Button onclick={() => session.chooseComputerFolder()}>Choose a folder for it…</Button>
    </div>
  {:else if offer.start}
    {#if offer.missingPath}
      <h1>Your library was not found</h1>
      <p class="where">
        Your settings say the library is in <code>{offer.missingPath}</code>, but nothing is there. If it is on a drive or
        folder that is not connected, connect it and press Rescan. Nothing was made.
      </p>
    {:else}
      <h1>Start your library</h1>
      <p>Your tracks, mixes and cases live in a library on this computer. You do not need the Pacemaker to work on them.</p>
    {/if}
    <div class="actions">
      {#if offer.missingPath}
        <Button variant="primary" onclick={() => session.chooseComputerFolder()} title="Pick the folder your library is in, or the place to make a new one">Choose where the library is…</Button>
        <Button onclick={() => session.startComputer()} title="Makes a new, empty library in {session.computer?.path}. Use it only if the first is gone.">Start a new, empty library in {session.computer?.path}</Button>
      {:else}
        <Button variant="primary" onclick={() => session.startComputer()}>Start a library on this computer</Button>
      {/if}
      {#if offer.copyFrom}
        <Button onclick={() => session.copyFromPacemaker(offer.copyFrom!)} title="Makes the library on this computer, then shows what would be copied from the Pacemaker before anything is">
          Copy my Pacemaker's library to this computer
        </Button>
      {/if}
    </div>
    {#if offer.missingPath}
      <p class="where">Starting a new library makes a second, empty one in that folder, so use it only if the first is gone.</p>
    {:else}
      <p class="where">
        It will be made in <code>{session.computer?.path}</code>
        <Button small variant="link" onclick={() => session.chooseComputerFolder()}>Change…</Button>
      </p>
    {/if}
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
    <Button onclick={rescan} title="Look for a connected Pacemaker, and for the library on this computer again">Rescan</Button>
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
  /* a button that names a long path wraps instead of running off the window */
  .actions :global(button) {
    max-width: 100%;
    white-space: normal;
    overflow-wrap: anywhere;
  }
  .where {
    font-size: 12px;
    overflow-wrap: anywhere;
  }
</style>
