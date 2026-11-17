<script lang="ts">
  import {
    deviceFirmwareInspect,
    deviceFirmwareInstall,
    JOB_FIRMWARE,
    pickFirmwareFile,
    type PfwInfo,
  } from "$lib/api/device";
  import { jobs } from "$lib/state/jobs.svelte";
  import { session } from "$lib/state/session.svelte";
  import { toast } from "$lib/state/toast.svelte";
  import Button from "$lib/ui/Button.svelte";
  import Modal from "$lib/ui/Modal.svelte";
  import Card from "./Card.svelte";
  import { device } from "./device.svelte";
  import JobProgress from "./JobProgress.svelte";
  import HelpLink from "../help/HelpLink.svelte";
  import { bytes } from "./format";

  let fw = $derived(device.overview?.firmware);
  let pending = $derived(fw?.pending_pfw ?? []);
  let picked = $state<{ path: string; info: PfwInfo } | null>(null);
  let inspecting = $state(false);

  // Reads the chosen file and shows what it is before anything is copied.
  async function choose() {
    const path = await pickFirmwareFile();
    if (!path) return;
    inspecting = true;
    try {
      picked = { path, info: await deviceFirmwareInspect(path) };
    } catch (e) {
      session.error = String(e);
    } finally {
      inspecting = false;
    }
  }

  async function install() {
    const p = picked;
    const lib = session.library;
    picked = null;
    if (!p || !lib) return;
    device.begin(JOB_FIRMWARE, "Copying the firmware file");
    try {
      await deviceFirmwareInstall(lib.root, p.path);
    } catch (e) {
      device.job = null;
      toast.error("Couldn't install the firmware.", e);
    }
  }

  let sameVersion = $derived(!!picked && picked.info.fw_version === fw?.fw_version);
</script>

<Card title="Firmware">
  {#snippet aside()}<HelpLink id="install-firmware" />{/snippet}
  {#if !fw}
    <p class="muted">Reading the drive…</p>
  {:else}
    <div class="field"><span class="k">Firmware</span><span class="num">{fw.fw_version ?? "unknown"}</span></div>
    <div class="field"><span class="k">Software</span><span class="num">{fw.sw_version ?? "unknown"}</span></div>
    <div class="field"><span class="k">Bootloader</span><span class="num">{fw.bootloader_version ?? "unknown"}</span></div>
    <div class="field"><span class="k">Serial</span><span class="num">{fw.serial_masked ?? "unknown"}</span></div>
    <div class="field">
      <span class="k">Editor flag</span><span class="num">{fw.editor_compatible_flag ?? "unknown"}</span>
    </div>
    {#if !fw.fw_version}<p class="note">No readable .Pacemaker/system.xml on this drive.</p>{/if}
  {/if}

  <div class="actions">
    <Button
      disabled={inspecting || device.blocked || !session.library}
      title={jobs.busy ? "Another job is still running" : undefined}
      onclick={choose}
    >
      {inspecting ? "Checking file…" : "Install firmware…"}
    </Button>
  </div>
  <JobProgress kind={JOB_FIRMWARE} />

  {#if pending.length > 0}
    <div class="steps" role="region" aria-label="Finish the firmware install">
      <b>Waiting on the device: {pending.join(", ")}</b>
      <p class="muted">
        Not installed yet. It installs at the next start, USB unplugged.
      </p>
      <ol>
        <li>Eject the Pacemaker.</li>
        <li>Unplug the USB cable. The install does not run while it is plugged in.</li>
        <li>Power it on and leave it until it restarts.</li>
      </ol>
      <p class="muted">A new firmware number resets the device settings.</p>
      <Button
        disabled={device.blocked || !session.library}
        title={jobs.busy ? "Another job is still running" : undefined}
        onclick={() => session.library && device.eject(session.library.root)}
      >
        {device.ejecting ? "Ejecting…" : "Eject now"}
      </Button>
    </div>
  {:else if device.results["device-firmware"]}
    <p class="note">{device.results["device-firmware"]}</p>
  {/if}
</Card>

{#if picked}
  {@const i = picked.info}
  <Modal title={i.problems.length ? "This file cannot be installed" : "Copy this firmware to the device?"} onclose={() => (picked = null)}>
    <div class="facts">
      <span class="k">File</span><span>{i.file_name}</span>
      <span class="k">Size</span><span class="num">{bytes(i.size)}</span>
      <span class="k">Firmware</span><span class="num">{i.fw_version || "?"}</span>
      <span class="k">Software</span><span class="num">{i.sw_version || "?"}</span>
      <span class="k">Bootloader</span><span class="num">{i.bootloader_version || "?"}</span>
      <span class="k">Checksum</span><span class={i.md5_ok ? "good" : "badtext"}>{i.md5_ok ? "matches" : "does not match"}</span>
      <span class="k">Installed now</span><span class="num">{fw?.fw_version ?? "unknown"}</span>
    </div>
    {#if i.problems.length}
      <ul class="issues bad">
        {#each i.problems as p (p)}<li>{p}</li>{/each}
      </ul>
    {:else}
      {#if sameVersion}
        <p class="muted">
          Same firmware number as installed. Nothing is flashed.
        </p>
      {/if}
      <p class="muted">
        Installs after eject, unplug and power on. A new firmware resets the settings.
      </p>
    {/if}
    {#if i.warnings.length}
      <ul class="issues">
        {#each i.warnings as w (w)}<li>{w}</li>{/each}
      </ul>
    {/if}
    {#snippet actions()}
      <Button data-autofocus onclick={() => (picked = null)}>Cancel</Button>
      {#if i.problems.length === 0}
        <Button variant="primary" onclick={install}>Copy to device</Button>
      {/if}
    {/snippet}
  </Modal>
{/if}

<style>
  .field {
    display: grid;
    grid-template-columns: 96px 1fr;
    gap: 6px;
    margin: 4px 0;
  }
  .k {
    color: var(--muted);
    font-size: 12px;
  }
  .actions {
    margin-top: 10px;
  }
  .note {
    font-size: 12px;
    color: var(--muted);
    border-top: 1px solid var(--line);
    margin-top: 12px;
    padding-top: 10px;
  }
  .steps {
    margin-top: 12px;
    padding: 10px;
    border: 1px solid var(--accent);
    border-radius: 6px;
    background: var(--accent-soft);
  }
  .steps p {
    margin: 6px 0;
    font-size: 12px;
  }
  .steps ol {
    margin: 6px 0;
    padding-left: 20px;
  }
  .steps li {
    padding: 2px 0;
  }
  .facts {
    display: grid;
    grid-template-columns: 100px 1fr;
    gap: 4px 8px;
    margin: 8px 0;
  }
  .good {
    color: var(--ok);
  }
  .badtext {
    color: var(--bad);
  }
  .issues {
    margin: 8px 0;
    padding-left: 18px;
    font-size: 12px;
  }
  .issues.bad {
    color: var(--bad-text);
    background: var(--bad-bg);
    border-radius: 4px;
    padding: 8px 8px 8px 24px;
  }
</style>
