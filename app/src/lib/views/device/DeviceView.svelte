<script lang="ts">
  import { untrack } from "svelte";
  import { session } from "$lib/state/session.svelte";
  import Button from "$lib/ui/Button.svelte";
  import { volumeName } from "$lib/format";
  import BackupCard from "./BackupCard.svelte";
  import CleanupModal from "./CleanupModal.svelte";
  import { device } from "./device.svelte";
  import FirmwareCard from "./FirmwareCard.svelte";
  import HealthCard from "./HealthCard.svelte";
  import SettingsCard from "./SettingsCard.svelte";
  import StorageCard from "./StorageCard.svelte";

  // Storage, health, firmware, backup and settings of the docked Pacemaker. Everything is read
  // when the view opens and again whenever the library is re-read (after an add or re-analysis).
  let lib = $derived(session.library);
  let detached = $derived(device.isDetached(lib));

  $effect(() => {
    const root = lib?.root;
    if (root && !untrack(() => device.isDetached(lib))) untrack(() => device.load(root));
  });
</script>

<div class="bar">
  <h2>Device</h2>
  {#if lib}
    <span class="chip">{volumeName(lib.root)}</span>
    <span class="sp"></span>
    <Button
      disabled={device.blocked || detached}
      title={device.blocked && !device.ejecting ? "A job is still writing to the device" : undefined}
      onclick={() => device.eject(lib.root)}
    >
      {device.ejecting ? "Ejecting…" : "Eject"}
    </Button>
  {/if}
</div>

{#if lib && detached}
  <p class="muted pad" role="status">
    {volumeName(lib.root)} was ejected. It is safe to unplug the USB cable. To work with a Pacemaker again, plug it in
    and open it from the drive list.
  </p>
{:else if lib}
  <div class="cols">
    <StorageCard />
    <HealthCard />
    <FirmwareCard />
    <BackupCard />
    <SettingsCard />
  </div>
  <CleanupModal />
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
</style>
