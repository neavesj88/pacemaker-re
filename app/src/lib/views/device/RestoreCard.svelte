<script lang="ts">
  import { deviceSnapshotRows, deviceSnapshotsOpen, type SnapshotRow, type SnapshotsPanel } from "$lib/api/device";
  import { session } from "$lib/state/session.svelte";
  import { toast } from "$lib/state/toast.svelte";
  import Button from "$lib/ui/Button.svelte";
  import Card from "./Card.svelte";
  import { device } from "./device.svelte";
  import { bytes, plural } from "./format";
  import RestoreModal from "./RestoreModal.svelte";
  import { ageText, diffSummary, stampLabel, stampToDate } from "./trustLogic";

  // The app's automatic copies of music.db, each with what a restore from it would change on the
  // card now. Restoring replaces music.db only, after a fresh copy of the current one.
  let lib = $derived(session.library);
  let panel = $state<SnapshotsPanel | null>(null);
  let error = $state("");
  let reviewing = $state<SnapshotRow | null>(null);
  const now = Date.now();

  $effect(() => {
    const root = lib?.root;
    // re-read after a job ends too: a first write may just have made a snapshot
    void device.job;
    void device.snapshotTick;
    if (!root) {
      panel = null;
      return;
    }
    error = "";
    deviceSnapshotRows(root).then(
      (p) => (panel = p),
      (e) => (error = String(e)),
    );
  });

  function age(r: SnapshotRow) {
    const d = stampToDate(r.stamp);
    return d ? ageText(d.getTime(), now) : "";
  }

  async function openFolder() {
    if (!lib) return;
    try {
      await deviceSnapshotsOpen(lib.root);
    } catch (e) {
      toast.show(String(e), "bad", 8000);
    }
  }
</script>

<Card title="Restore from a snapshot">
  <p class="muted">
    Before the first change to this library in each session the app copies music.db to its own data folder (never onto
    the Pacemaker) and keeps the last {panel?.keep ?? 10}. If the copy fails, nothing is written. Restoring puts one of
    those copies back as the card's music.db: tracks, cases and mixes go back to how they were then. Audio files, beat
    grids and the device settings are not touched.
  </p>
  {#if error}
    <p class="bad">{error}</p>
  {:else if !panel}
    <p class="muted">Reading the snapshots…</p>
  {:else if panel.rows.length === 0}
    <p class="muted">None yet: the first change in this session makes one.</p>
  {:else}
    <ul class="rows" aria-label="Snapshots">
      {#each panel.rows as r (r.name)}
        <li class:broken={!!r.problem}>
          <div class="when">
            <b>{stampLabel(r.stamp)}</b>
            <span class="muted">{age(r)}</span>
          </div>
          {#if r.problem}
            <div class="what bad">Cannot be restored: {r.problem}.</div>
          {:else}
            <div class="what">
              <span class="num">
                {plural(r.snapshot?.tracks ?? 0, "track")}, {plural(r.snapshot?.cases ?? 0, "case")}, {plural(r.snapshot?.mixes ?? 0, "mix", "mixes")}
              </span>
              <span class="muted"> · {bytes(r.music_db_bytes)}</span>
              <div class="diff">{diffSummary(r.counts)}</div>
            </div>
          {/if}
          <Button
            small
            disabled={!!r.problem || device.blocked || !lib}
            title={device.blocked ? "A job is still writing to the device" : undefined}
            onclick={() => (reviewing = r)}>Review…</Button
          >
        </li>
      {/each}
    </ul>
    <div class="foot">
      <p class="path">{panel.folder}</p>
      <Button small onclick={openFolder}>Open folder</Button>
    </div>
  {/if}
</Card>

{#if reviewing && lib}
  <RestoreModal row={reviewing} onclose={() => (reviewing = null)} />
{/if}

<style>
  .rows {
    list-style: none;
    margin: 8px 0 0;
    padding: 0;
    border: 1px solid var(--line);
    border-radius: 4px;
  }
  li {
    display: grid;
    grid-template-columns: 170px 1fr auto;
    gap: 8px;
    align-items: start;
    padding: 6px 8px;
  }
  li:nth-child(odd) {
    background: var(--row);
  }
  .when {
    display: flex;
    flex-direction: column;
    gap: 1px;
  }
  .when .muted {
    font-size: 11px;
  }
  .diff {
    font-size: 12px;
    color: var(--muted);
    margin-top: 2px;
  }
  .what {
    min-width: 0;
    overflow-wrap: anywhere;
  }
  .bad {
    color: var(--bad-text);
    font-size: 12px;
  }
  .foot {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    margin-top: 8px;
  }
  .path {
    font-size: 12px;
    overflow-wrap: anywhere;
    margin: 0;
    color: var(--muted);
  }
</style>
