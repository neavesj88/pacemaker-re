<script lang="ts">
  import {
    deviceBackup,
    deviceVerifyBackup,
    JOB_BACKUP,
    JOB_VERIFY,
    pickBackupFolder,
    pickVerifyFolder,
  } from "$lib/api/device";
  import { jobs } from "$lib/state/jobs.svelte";
  import { session } from "$lib/state/session.svelte";
  import { toast } from "$lib/state/toast.svelte";
  import Button from "$lib/ui/Button.svelte";
  import Modal from "$lib/ui/Modal.svelte";
  import HelpLink from "../help/HelpLink.svelte";
  import Card from "./Card.svelte";
  import { device } from "./device.svelte";
  import JobProgress from "./JobProgress.svelte";
  import { bytes, plural } from "./format";
  import VerifyModal from "./VerifyModal.svelte";

  let includeAudio = $state(false);
  let dest = $state<string | null>(null);
  let lib = $derived(session.library);
  let audioBytes = $derived(device.overview?.storage.music_bytes ?? 0);

  async function choose() {
    const d = await pickBackupFolder();
    if (d) dest = d;
  }

  async function verify() {
    const folder = await pickVerifyFolder();
    if (!folder) return;
    device.begin(JOB_VERIFY, "Starting the check");
    try {
      await deviceVerifyBackup(folder);
    } catch (e) {
      device.job = null;
      toast.error("Couldn't check the backup.", e);
    }
  }

  async function start() {
    const d = dest;
    dest = null;
    if (!d || !lib) return;
    device.begin(JOB_BACKUP, "Starting the backup");
    try {
      await deviceBackup(lib.root, d, includeAudio);
    } catch (e) {
      device.job = null;
      toast.error("Couldn't back up.", e);
    }
  }
</script>

<Card title="Backup" id="device-backup-card">
  {#snippet aside()}<HelpLink id="device-page" anchor="backup" />{/snippet}
  <p class="muted">
    Database, settings, system file and beat grids.
  </p>
  <label class="check">
    <input type="checkbox" bind:checked={includeAudio} />
    Include the audio files{audioBytes ? ` (${bytes(audioBytes)})` : ""}
  </label>
  <div class="actions">
    <Button
      variant="primary"
      disabled={!lib || device.blocked}
      title={jobs.busy ? "Another job is still running" : undefined}
      onclick={choose}>Back up…</Button
    >
  </div>
  <JobProgress kind={JOB_BACKUP} />
  {#if device.results[JOB_BACKUP]}<p class="note">{device.results[JOB_BACKUP]}</p>{/if}

  <div class="auto">
    <h4>Check a backup</h4>
    <p class="muted">
      Checks a backup folder against its SHA-256 list.
    </p>
    <div class="actions">
      <Button disabled={!lib || device.blocked} onclick={verify}>Verify a backup…</Button>
      {#if device.verify}<Button variant="ghost" onclick={() => (device.verifyOpen = true)}>Show the files</Button>{/if}
    </div>
    <JobProgress kind={JOB_VERIFY} />
    {#if device.results[JOB_VERIFY]}<p class="note">{device.results[JOB_VERIFY]}</p>{/if}
  </div>
</Card>

{#if device.verifyOpen && device.verify}
  <VerifyModal report={device.verify} onclose={() => (device.verifyOpen = false)} />
{/if}

{#if dest && lib}
  <Modal title="Back up the Pacemaker?" onclose={() => (dest = null)}>
    <div class="facts">
      <span class="k">From</span><span>{lib.root}</span>
      <span class="k">Into</span><span>{dest}</span>
      <span class="k">Contents</span>
      <span>
        music.db, settings, system file and the beat grids of {plural(lib.tracks.length, "track")}{includeAudio
          ? `, plus all audio (${bytes(audioBytes)})`
          : ", no audio"}
      </span>
    </div>
    <p class="muted">
      Creates a dated folder. The Pacemaker is not changed.
    </p>
    {#snippet actions()}
      <Button data-autofocus onclick={() => (dest = null)}>Cancel</Button>
      <Button variant="primary" onclick={start}>Back up</Button>
    {/snippet}
  </Modal>
{/if}

<style>
  .check {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 8px 0;
  }
  .actions {
    margin-top: 6px;
  }
  .auto {
    border-top: 1px solid var(--line);
    margin-top: 12px;
    padding-top: 10px;
  }
  .auto h4 {
    margin: 0 0 4px;
    font-size: 13px;
  }
  .note {
    font-size: 12px;
    color: var(--muted);
    border-top: 1px solid var(--line);
    margin-top: 12px;
    padding-top: 10px;
    overflow-wrap: anywhere;
  }
  .facts {
    display: grid;
    grid-template-columns: 70px 1fr;
    gap: 4px 8px;
    margin: 8px 0;
    overflow-wrap: anywhere;
  }
  .k {
    color: var(--muted);
    font-size: 12px;
  }
</style>
