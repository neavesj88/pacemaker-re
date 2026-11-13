<script lang="ts">
  import { untrack } from "svelte";
  import { nav } from "$lib/state/nav.svelte";
  import { session } from "$lib/state/session.svelte";
  import Button from "$lib/ui/Button.svelte";
  import { volumeName } from "$lib/format";
  import ComputerLibrarySettings from "../settings/ComputerLibrarySettings.svelte";
  import BackupCard from "./BackupCard.svelte";
  import CleanupModal from "./CleanupModal.svelte";
  import { device } from "./device.svelte";
  import FirmwareCard from "./FirmwareCard.svelte";
  import FixModal from "./FixModal.svelte";
  import HealthCard from "./HealthCard.svelte";
  import PresetsCard from "./PresetsCard.svelte";
  import RelinkModal from "./RelinkModal.svelte";
  import RestoreCard from "./RestoreCard.svelte";
  import SafetyCard from "./SafetyCard.svelte";
  import SettingsCard from "./SettingsCard.svelte";
  import StorageCard from "./StorageCard.svelte";
  import UndoCard from "./UndoCard.svelte";
  import VocalCard from "./VocalCard.svelte";

  // Storage, health, firmware, backup and settings of the docked Pacemaker. Everything is read
  // when the view opens and again whenever the library is re-read (after an add or re-analysis).
  //
  // A library on this computer has no storage, firmware, vocal-filter flag, settings.dat or eject: those
  // cards are not shown, only the ones that are about the library itself (health, snapshots, the edit
  // history, the safety settings), under a note saying where the library is.
  let lib = $derived(session.library);
  let onComputer = $derived(session.isComputer);
  let detached = $derived(!onComputer && device.isDetached(lib));

  $effect(() => {
    const root = lib?.root;
    // (for a library on this computer `load` reads only the health: there is no card to ask)
    if (root && !untrack(() => device.isDetached(lib))) untrack(() => device.load(root));
  });
</script>

<div class="bar">
  <h2>Device</h2>
  {#if lib}
    <span class="chip">{onComputer ? "This computer" : volumeName(lib.root)}</span>
    <span class="sp"></span>
    {#if !onComputer}
      <Button
        disabled={device.blocked || detached}
        title={device.blocked && !device.ejecting ? "A job is still writing to the device" : undefined}
        onclick={() => device.eject(lib.root)}
      >
        {device.ejecting ? "Ejecting…" : "Eject"}
      </Button>
    {/if}
  {/if}
</div>

{#if lib && detached}
  <p class="muted pad" role="status">
    {volumeName(lib.root)} was ejected. It is safe to unplug the USB cable. To work with a Pacemaker again, plug it in
    and open it from the drive list.
  </p>
{:else if lib && onComputer}
  <div class="cols">
    <section class="here">
      <h4>Your Pacemaker</h4>
      <p>
        This library is on this computer at <code>{lib.root}</code>. Plug in your Pacemaker (or put its card in a reader) to
        send tracks, mixes and cases to it. Only a drive root (such as <code>G:\</code>) or a mount point counts as a Pacemaker; a
        card image copied into a folder is opened as a library on this computer, so the card-only tools are not offered for it.
      </p>
      <Button variant="primary" onclick={() => nav.go("sync")}>Sync with a Pacemaker…</Button>
      <h4 class="sub">Where this library is kept</h4>
      <ComputerLibrarySettings />
    </section>
    <HealthCard />
    <RestoreCard />
    <UndoCard />
    <SafetyCard />
  </div>
  <CleanupModal />
  {#if device.fixOpen}<FixModal />{/if}
  {#if device.relinkOpen}<RelinkModal />{/if}
{:else if lib}
  <div class="cols">
    <StorageCard />
    <HealthCard />
    <FirmwareCard />
    <VocalCard />
    <BackupCard />
    <RestoreCard />
    <UndoCard />
    <SafetyCard />
    <SettingsCard />
    <PresetsCard />
  </div>
  <CleanupModal />
  {#if device.fixOpen}<FixModal />{/if}
  {#if device.relinkOpen}<RelinkModal />{/if}
{:else}
  <p class="muted pad">Open a Pacemaker drive to see its storage, firmware and settings.</p>
{/if}

<style>
  .bar {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 7px 12px;
    border-bottom: 1px solid var(--line);
  }
  h2 {
    margin: 0 8px 0 0;
    font-size: 15px;
  }
  .chip {
    font-size: 11px;
    padding: 1px 7px;
    border-radius: 9px;
    background: color-mix(in srgb, var(--green) 10%, transparent);
    color: var(--green);
  }
  .sp {
    flex: 1;
  }
  .cols {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(380px, 1fr));
    gap: 14px;
    padding: 12px;
    align-items: start;
  }
  .pad {
    padding: 20px;
  }
  .here {
    background: var(--panel);
    border: 1px solid var(--line);
    border-radius: 8px;
    padding: 12px;
    grid-column: 1 / -1;
  }
  .here h4 {
    margin: 0 0 8px;
  }
  .here .sub {
    margin: 16px 0 8px;
  }
  .here p {
    margin: 0 0 10px;
    overflow-wrap: anywhere;
  }
</style>
