<script lang="ts">
  // The warning before any Revival firmware is copied to the Pacemaker: "Update to <number>" and the
  // Install firmware... flow both open it. Copy firmware stays disabled until both boxes are ticked.
  import Button from "$lib/ui/Button.svelte";
  import Modal from "$lib/ui/Modal.svelte";
  import HelpLink from "../help/HelpLink.svelte";
  import { device } from "./device.svelte";
  import { backupStamp, canCopy } from "./firmwareLogic";

  let { fileName, fwVersion, same = false, onconfirm, oncancel }: { fileName: string; fwVersion: string; same?: boolean; onconfirm: () => void; oncancel: () => void } = $props();

  let backup = $state(false);
  let risk = $state(false);

  // Closes the dialog and brings the Backup card into view.
  function toBackup() {
    oncancel();
    queueMicrotask(() => document.getElementById("device-backup-card")?.scrollIntoView({ block: "center" }));
  }
</script>

<Modal title="Install firmware {fwVersion}?" onclose={oncancel}>
  <p class="muted file">{fileName}</p>
  {#if same}<p class="muted file">Same number as installed: nothing is flashed.</p>{/if}

  <label class="check">
    <input type="checkbox" bind:checked={backup} />
    I have a verified backup of this Pacemaker
  </label>
  <p class="sub muted">
    {#if device.lastBackupAt}
      Last backup this session: {backupStamp(device.lastBackupAt)}.
    {:else}
      No backup yet this session. <button type="button" class="link" onclick={toBackup}>Go to the Backup card</button>
    {/if}
  </p>

  <label class="check">
    <input type="checkbox" bind:checked={risk} />
    I understand this firmware is untested, comes with no warranty and no recourse, and I install it at my own risk
  </label>

  <p class="best">Works best with the microSD storage upgrade. <HelpLink id="storage-upgrade" /></p>

  <p class="fail"><b>If it does not start:</b></p>
  <ol>
    <li>Put revival-no-preload on the card (Firmware card: Add recovery file).</li>
    <li>Hold Play + Cue + Forward + Back at power-on for recovery mode.</li>
    <li>DFU recovery: see Ways to install firmware. <HelpLink id="firmware-methods" anchor="recover-with-dfu" /></li>
  </ol>

  {#snippet actions()}
    <Button data-autofocus onclick={oncancel}>Cancel</Button>
    <Button variant="primary" disabled={!canCopy(backup, risk)} onclick={onconfirm}>Copy firmware</Button>
  {/snippet}
</Modal>

<style>
  .file {
    margin: 0 0 10px;
    font-size: 12px;
    overflow-wrap: anywhere;
  }
  .check {
    display: flex;
    align-items: baseline;
    gap: 8px;
    margin: 8px 0 2px;
  }
  .sub {
    margin: 0 0 6px 24px;
    font-size: 12px;
  }
  .link {
    padding: 0;
    border: 0;
    background: none;
    color: var(--accent);
    font: inherit;
    text-decoration: underline;
    cursor: pointer;
  }
  .best {
    margin: 12px 0 8px;
  }
  .fail {
    margin: 8px 0 2px;
  }
  ol {
    margin: 4px 0 8px;
    padding-left: 20px;
  }
  li {
    padding: 2px 0;
  }
</style>
