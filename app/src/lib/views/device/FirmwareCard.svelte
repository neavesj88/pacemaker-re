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
  import { vocal } from "$lib/state/vocal.svelte";
  import Button from "$lib/ui/Button.svelte";
  import Modal from "$lib/ui/Modal.svelte";
  import Card from "./Card.svelte";
  import { device } from "./device.svelte";
  import JobProgress from "./JobProgress.svelte";
  import { firmwareOffer } from "./firmware.svelte";
  import { bundledLabel, fwNumber, isRevivalFirmware, offerUpdate, recoveryButton, recoveryState } from "./firmwareLogic";
  import FirmwareWarning from "./FirmwareWarning.svelte";
  import HelpLink from "../help/HelpLink.svelte";
  import { bytes } from "./format";

  let fw = $derived(device.overview?.firmware);
  let pending = $derived(fw?.pending_pfw ?? []);
  let picked = $state<{ path: string; info: PfwInfo } | null>(null);
  let inspecting = $state(false);
  /** A Revival file waiting for the warning dialog's Copy firmware. */
  let warn = $state<{ path: string; info: PfwInfo } | null>(null);
  let recoveryBusy = $state(false);

  // the file the app ships is read from the app's own folder; the recovery file is read from the card
  // the way the Vocal card does (the user opened this library by clicking it)
  $effect(() => {
    void firmwareOffer.load();
  });
  $effect(() => {
    const root = session.library?.root;
    if (root) void vocal.load(root);
  });

  let bundled = $derived(firmwareOffer.bundled);
  let offered = $derived(offerUpdate(bundled?.fw_version ?? null, fw?.fw_version, firmwareOffer.offer));
  let recoveryOn = $derived(vocal.state ? vocal.state.kill_switch : null);

  async function toggleRecovery() {
    const lib = session.library;
    if (!lib || recoveryOn === null) return;
    recoveryBusy = true;
    try {
      await vocal.setRecovery(lib.root, !recoveryOn);
      toast.show(recoveryOn ? "Recovery file removed." : "Recovery file added. Revival's add-ons stay off at the next start.", "ok");
    } catch (e) {
      toast.error("Couldn't change the recovery file.", e);
    } finally {
      recoveryBusy = false;
    }
  }

  // Inspects the file that ships with the app, then asks first like every Revival install.
  async function updateBundled() {
    if (!bundled) return;
    inspecting = true;
    try {
      const info = await deviceFirmwareInspect(bundled.path);
      if (info.problems.length) toast.error("The included firmware cannot be installed.", info.problems.join("; "));
      else warn = { path: bundled.path, info };
    } catch (e) {
      toast.error("Couldn't check the included firmware.", e);
    } finally {
      inspecting = false;
    }
  }

  // Any Revival file goes through the warning; other files are copied as before.
  function confirmCopy() {
    const p = picked;
    if (!p) return;
    if (isRevivalFirmware(p.info.file_name, p.info.fw_version)) {
      picked = null;
      warn = p;
    } else void install(p);
  }

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

  async function install(p: { path: string; info: PfwInfo } | null) {
    const lib = session.library;
    picked = null;
    warn = null;
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
    <div class="field"><span class="k">On the Pacemaker</span><span class="num">{fw.fw_version ?? "unknown"}</span></div>
    {#if bundled}
      <div class="field"><span class="k">Included with the app</span><span class="num">{bundled.fw_version}</span></div>
    {/if}
    <div class="field"><span class="k">Software</span><span class="num">{fw.sw_version ?? "unknown"}</span></div>
    <div class="field"><span class="k">Bootloader</span><span class="num">{fw.bootloader_version ?? "unknown"}</span></div>
    <div class="field"><span class="k">Serial</span><span class="num">{fw.serial_masked ?? "unknown"}</span></div>
    <div class="field">
      <span class="k">Editor flag</span><span class="num">{fw.editor_compatible_flag ?? "unknown"}</span>
    </div>
    {#if !fw.fw_version}<p class="note">No readable .Pacemaker/system.xml on this drive.</p>{/if}
  {/if}

  {#if offered && bundled}
    <p class="update">Update available. Settings reset after updating.</p>
  {/if}
  <div class="actions">
    {#if bundled}
      <Button
        variant={offered ? "primary" : undefined}
        disabled={inspecting || device.blocked || !session.library}
        title={jobs.busy ? "Another job is still running" : undefined}
        onclick={updateBundled}
      >
        {bundledLabel(bundled.fw_version, fw?.fw_version, offered)}
      </Button>
    {/if}
    <Button
      disabled={inspecting || device.blocked || !session.library}
      title={jobs.busy ? "Another job is still running" : undefined}
      onclick={choose}
    >
      {inspecting ? "Checking file…" : "Install firmware…"}
    </Button>
  </div>
  <JobProgress kind={JOB_FIRMWARE} />

  {#if fw}
    <div class="field recovery">
      <span class="k">Recovery file</span>
      <span class="rec">
        <span>{recoveryOn === null ? "Reading…" : recoveryState(recoveryOn)}</span>
        <Button
          small
          disabled={recoveryOn === null || recoveryBusy || device.blocked || !session.library}
          title={device.blocked ? "A job is still writing to the device" : undefined}
          onclick={toggleRecovery}
        >
          {recoveryBusy ? "Saving…" : recoveryButton(!!recoveryOn)}
        </Button>
      </span>
    </div>
    <p class="hint">Turns off Revival's add-ons at the next start.</p>
  {/if}

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
      <p class="muted">Settings reset after the install.</p>
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
          Same number: only the app image is replaced.
        </p>
      {/if}
      <p class="muted">
        Installs after eject, unplug and power on. Settings reset.
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
        <Button variant="primary" onclick={confirmCopy}>Copy to device</Button>
      {/if}
    {/snippet}
  </Modal>
{/if}

{#if warn}
  {@const w = warn}
  <FirmwareWarning fileName={w.info.file_name} fwVersion={w.info.fw_version} same={fwNumber(w.info.fw_version) !== null && fwNumber(w.info.fw_version) === fwNumber(fw?.fw_version)} onconfirm={() => install(w)} oncancel={() => (warn = null)} />
{/if}

<style>
  .field {
    display: grid;
    grid-template-columns: 130px 1fr;
    gap: 6px;
    margin: 4px 0;
  }
  .k {
    color: var(--muted);
    font-size: 12px;
  }
  .actions {
    margin-top: 10px;
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }
  .update {
    margin: 10px 0 0;
    font-size: 12px;
    color: var(--accent);
  }
  .rec {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }
  .recovery {
    margin-top: 12px;
  }
  .hint {
    margin: 2px 0 0;
    font-size: 12px;
    color: var(--muted);
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
