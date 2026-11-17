<script lang="ts">
  import { volumeName } from "$lib/format";
  import { session } from "$lib/state/session.svelte";
  import { welcomeOffer } from "$lib/state/place";
  import Button from "$lib/ui/Button.svelte";
  import RMark from "$lib/ui/RMark.svelte";

  const offer = $derived(welcomeOffer(session.computer, session.volumes, session.pickedPath));

  // a library on a drive that was not connected a moment ago may be there now
  // Rescan with nothing to show must still say it looked
  let checkedAt = $state(0);
  const rescan = async () => {
    await Promise.all([session.refreshVolumes(), session.refreshComputer()]);
    checkedAt = Date.now();
  };
  const checkedTime = $derived(new Date(checkedAt).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", second: "2-digit" }));
</script>

<div class="welcome">
  <RMark size={96} />
  {#if offer.unreadable}
    <h1>Your library settings could not be read</h1>
    <p>
      Nothing was opened. Open a folder or a Pacemaker below, or use the default location{session.computer?.default_path ? ` (${session.computer.default_path})` : ""}. The damaged file is kept as <code>.bad</code>.
    </p>
    <div class="actions">
      <Button variant="primary" onclick={() => session.saveComputer({ path: "", open_at_start: true })} title="Replaces the damaged settings with the defaults.">
        Use the default location
      </Button>
      <Button onclick={() => session.chooseComputerFolder()}>Choose a folder for it…</Button>
    </div>
  {:else if offer.start}
    {#if offer.emptyFolder}
      <h1>Welcome. Where should your library live?</h1>
      <p>Your library lives on this computer. No Pacemaker needed.</p>
    {:else if offer.missingPath}
      <h1>Your library was not found</h1>
      <p class="where">
        Nothing is in <code>{offer.missingPath}</code>. If the drive is not connected, connect it and press Rescan.
      </p>
    {:else}
      <h1>Start your library</h1>
      <p>Your library lives on this computer. No Pacemaker needed.</p>
    {/if}
    <div class="actions">
      {#if offer.emptyFolder}
        <Button variant="primary" onclick={() => session.startComputer()}>Start a new library in {session.computer?.path}</Button>
        <Button onclick={() => session.chooseComputerFolder()} title="Pick a different folder for the library">Choose a different folder…</Button>
      {:else if offer.missingPath}
        <Button variant="primary" onclick={() => session.chooseComputerFolder()} title="Pick the folder your library is in, or the place to make a new one">Choose where the library is…</Button>
        <Button onclick={() => session.startComputer()} title="Makes an empty library in {session.computer?.path}.">Start a new, empty library in {session.computer?.path}</Button>
      {:else}
        <Button variant="primary" onclick={() => session.startComputer()}>Start a library on this computer</Button>
      {/if}
      {#if offer.copyFrom}
        <Button onclick={() => session.copyFromPacemaker(offer.copyFrom!)} title="Creates the library here, then shows what would be copied.">
          Copy my Pacemaker's library to this computer
        </Button>
      {/if}
    </div>
    {#if offer.missingPath && !offer.emptyFolder && checkedAt}
      <p class="where" role="status" data-testid="welcome-checked">Still not found in <code>{offer.missingPath}</code> (checked at {checkedTime}).</p>
    {/if}
    {#if offer.emptyFolder}
      <!-- the folder is named on the main button; nothing else to warn about -->
    {:else if offer.missingPath}
      <p class="where">A new library is a second, empty one. Use it only if the first is gone.</p>
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
