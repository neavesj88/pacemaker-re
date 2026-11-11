<script lang="ts">
  import {
    deviceBackup,
    deviceSnapshots,
    deviceSnapshotsOpen,
    JOB_BACKUP,
    pickBackupFolder,
    type SnapshotsView,
  } from "$lib/api/device";
  import { jobs } from "$lib/state/jobs.svelte";
  import { session } from "$lib/state/session.svelte";
  import { toast } from "$lib/state/toast.svelte";
  import Button from "$lib/ui/Button.svelte";
  import Modal from "$lib/ui/Modal.svelte";
  import Card from "./Card.svelte";
  import { device } from "./device.svelte";
  import JobProgress from "./JobProgress.svelte";
  import { bytes, plural } from "./format";

  let includeAudio = $state(false);
  let dest = $state<string | null>(null);
  let lib = $derived(session.library);
  let audioBytes = $derived(device.overview?.storage.music_bytes ?? 0);

  // The automatic music.db safety copies (made before the first write of each session).
  let snaps = $state<SnapshotsView | null>(null);
  let snapsError = $state("");
  $effect(() => {
    const root = lib?.root;
    // re-read after a job ends too: a first write may just have made one
    void device.job;
    snaps = null;
    snapsError = "";
    if (!root) return;
    deviceSnapshots(root).then(
      (s) => (snaps = s),
      (e) => (snapsError = String(e)),
    );
  });

  async function openSnaps() {
    if (!lib) return;
    try {
      await deviceSnapshotsOpen(lib.root);
    } catch (e) {
      toast.show(String(e), "bad", 8000);
    }
  }

  async function choose() {
    const d = await pickBackupFolder();
    if (d) dest = d;
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
      toast.show(String(e), "bad", 8000);
    }
  }
</script>

<Card title="Backup">
  <p class="muted">
    A snapshot of the library: the database (tracks, mixes, cases), the device settings, the system file and every
    beat grid. Audio is left out unless you tick the box. Each file is read back and checked, and a SHA-256 list is
    saved beside the copy.
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
    <h4>Automatic snapshots</h4>
    <p class="muted">
      Before the first change to this library in each session the app copies music.db and the settings to its own data
      folder (never onto the Pacemaker) and keeps the last {snaps?.keep ?? 10}. If the copy fails, nothing is written.
    </p>
    {#if snapsError}
      <p class="bad">{snapsError}</p>
    {:else if snaps}
      <p class="muted">
        {snaps.snapshots.length === 0
          ? "None yet: the first change in this session makes one."
          : `${plural(snaps.snapshots.length, "snapshot")}, newest ${snaps.snapshots[0].name.replace("music-db-", "")}`}
      </p>
      <p class="path">{snaps.folder}</p>
      <Button small onclick={openSnaps}>Open folder</Button>
    {/if}
  </div>
</Card>

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
      A new folder named pacemaker-backup and the date is created inside the chosen folder. Nothing on the Pacemaker is
      changed or deleted.
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
  .path {
    font-size: 12px;
    overflow-wrap: anywhere;
    margin: 4px 0 8px;
  }
  .bad {
    color: var(--bad);
    font-size: 12px;
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
