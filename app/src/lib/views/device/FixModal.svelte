<script lang="ts">
  import { deviceFixPlan, deviceFixRemoveRows, type FixPlan } from "$lib/api/device";
  import { reanalyseTracks, tracksMixesUsing } from "$lib/api/tracks";
  import { jobs } from "$lib/state/jobs.svelte";
  import { session } from "$lib/state/session.svelte";
  import { toast } from "$lib/state/toast.svelte";
  import Button from "$lib/ui/Button.svelte";
  import { device } from "./device.svelte";
  import { plural } from "./format";
  import { relink } from "./relink.svelte";
  import { deadRowLine, massWarning, removalReport } from "./trustLogic";
  import WideModal from "./WideModal.svelte";

  // The Health card's fixes, each with the names of everything it would touch:
  //  - library rows whose audio file is gone (removed from music.db and from their cases; the
  //    files are not touched; a copy of music.db is saved first, so a restore brings them back)
  //  - tracks with audio but no usable beat grid (the existing re-analysis job, for exactly these)
  // The plan comes from the same classification the removal repeats on the fresh database.
  const root = session.library?.root ?? "";

  let plan = $state<FixPlan | null>(null);
  let error = $state("");
  let busy = $state(false);
  let unticked = $state<Set<number>>(new Set());
  let acknowledged = $state(false);

  $effect(() => {
    deviceFixPlan(root).then(
      (p) => (plan = p),
      (e) => (error = String(e)),
    );
  });

  const chosen = $derived((plan?.dead_rows ?? []).filter((r) => !unticked.has(r.track_id)));
  const needsAck = $derived(!!plan?.mass);
  const warning = $derived(plan ? massWarning(plan) : "");
  const nothing = $derived(
    !!plan && plan.dead_rows.length === 0 && plan.kept_for_mixes.length === 0 && plan.grids.length === 0 && plan.unchecked.length === 0,
  );

  /** The audio may still exist somewhere: look for it before giving up the rows. */
  function findFiles() {
    close();
    void relink.open();
  }

  function toggle(id: number) {
    const next = new Set(unticked);
    if (next.has(id)) next.delete(id);
    else next.add(id);
    unticked = next;
  }

  function close() {
    device.fixOpen = false;
  }

  async function remove() {
    if (busy || chosen.length === 0 || (needsAck && !acknowledged)) return;
    busy = true;
    try {
      const rep = await deviceFixRemoveRows(root, chosen.map((r) => r.track_id), acknowledged);
      close();
      toast.show(removalReport(rep), rep.skipped.length ? "bad" : "ok", 12000);
      await session.load(root, true);
      device.snapshotTick++;
      await device.load(root);
    } catch (e) {
      error = String(e);
      busy = false;
    }
  }

  /** Hands the tracks without a usable beat grid to the usual confirmation, which names each one. */
  async function reanalyse() {
    const p = plan;
    if (!p || p.grids.length === 0) return;
    const ids = p.grids.map((g) => g.track_id);
    let mixNote = "";
    try {
      const mixes = await tracksMixesUsing(root, ids);
      if (mixes.length > 0) {
        mixNote = ` ${mixes.length === 1 ? "1 mix uses" : `${mixes.length} mixes use`} some of these tracks (${mixes.join(", ")}): a mix clip keeps the tempo the track had when the mix was made, so ${mixes.length === 1 ? "that mix" : "those mixes"} may drift out of step until beatmatched again.`;
      }
    } catch (e) {
      mixNote = ` Could not check which mixes use these tracks (${String(e)}).`;
    }
    close();
    jobs.ask({
      kind: "reanalyse",
      root,
      count: ids.length,
      names: p.grids.map((g) => `${g.label}: ${g.reason}`),
      title: `Re-analyse ${plural(ids.length, "track")} on ${root} ?`,
      description:
        "Only the beat grid (.str) and BPM of these tracks are rewritten, from the audio already on the device. No audio is copied." + mixNote,
      confirmLabel: `Analyse ${ids.length}`,
      runningLabel: "Analysing",
      doneLabel: "Analysis finished",
      start: () => reanalyseTracks(root, ids),
    });
  }
</script>

<WideModal title="Fix library problems" onclose={close} width={720}>
  {#if error}
    <p class="err" role="alert">{error}</p>
  {:else if !plan}
    <p class="muted">Checking every track…</p>
  {:else}
    {#if !plan.music_folder_ok}
      <p class="err">
        The Music folder (.Pacemaker/Music) is missing or not a folder on this drive, so no track can be judged dead.
        Nothing is offered for removal.
      </p>
    {/if}
    {#if nothing}<p class="muted">Nothing to fix: every track has its audio and a usable beat grid.</p>{/if}

    {#if plan.dead_rows.length > 0}
      <h3>Library rows without an audio file ({plan.dead_rows.length})</h3>
      <p class="muted">
        Ticked rows are removed from the library, and from the cases named beside them. The audio is already gone;
        nothing on the drive is deleted. A copy of music.db is saved first, so Restore from a snapshot brings the rows
        back.
      </p>
      <p class="muted">
        If the files still exist somewhere (a backup, the folder they were added from), relinking is better than
        removing the rows.
        <Button small variant="link" disabled={device.blocked} onclick={findFiles}>Look for the files…</Button>
      </p>
      {#if warning}<p class="err" role="alert">{warning}</p>{/if}
      <ul class="list" aria-label="Rows to remove">
        {#each plan.dead_rows as r (r.track_id)}
          <li>
            <label>
              <input type="checkbox" checked={!unticked.has(r.track_id)} onchange={() => toggle(r.track_id)} />
              <span class="name">{deadRowLine(r)}</span>
              <span class="loc muted">{r.location}</span>
            </label>
          </li>
        {/each}
      </ul>
      {#if needsAck}
        <label class="ack">
          <input type="checkbox" bind:checked={acknowledged} />
          I have checked that this is the right drive and that it is fully mounted
        </label>
      {/if}
    {/if}

    {#if plan.kept_for_mixes.length > 0}
      <h3>Kept: used by a mix ({plan.kept_for_mixes.length})</h3>
      <p class="muted">These have no audio file either, but removing them would leave a hole in a mix, so they stay.</p>
      <ul class="list" aria-label="Rows kept for mixes">
        {#each plan.kept_for_mixes as r (r.track_id)}
          <li class="row"><span class="name">{r.label}</span><span class="loc muted">in {r.in_mixes.join(", ")}</span></li>
        {/each}
      </ul>
    {/if}

    {#if plan.unchecked.length > 0}
      <h3>Not checked ({plan.unchecked.length})</h3>
      <p class="muted">
        Their stored location is not a plain path under the Music folder, so the app does not look for them. Nothing is
        offered for these.
      </p>
      <ul class="list" aria-label="Tracks not checked">
        {#each plan.unchecked as r (r.track_id)}
          <li class="row"><span class="name">{r.label}</span><span class="loc muted">{r.location}</span></li>
        {/each}
      </ul>
    {/if}

    {#if plan.grids.length > 0}
      <h3>Audio without a usable beat grid ({plan.grids.length})</h3>
      <p class="muted">
        Re-analysing writes a new .str and BPM for exactly these tracks from the audio on the drive. You are asked to
        confirm, with this list, before it starts.
      </p>
      <ul class="list" aria-label="Tracks to re-analyse">
        {#each plan.grids as g (g.track_id)}
          <li class="row"><span class="name">{g.label}</span><span class="loc muted">{g.reason}</span></li>
        {/each}
      </ul>
    {/if}
  {/if}

  {#snippet actions()}
    <Button data-autofocus onclick={close}>Close</Button>
    {#if plan && plan.grids.length > 0}
      <Button disabled={device.blocked || busy} onclick={reanalyse}>Re-analyse {plural(plan.grids.length, "track")}…</Button>
    {/if}
    {#if plan && plan.dead_rows.length > 0}
      <Button
        variant="primary"
        disabled={chosen.length === 0 || busy || (needsAck && !acknowledged) || device.blocked}
        onclick={remove}
      >
        {busy ? "Removing…" : `Remove ${plural(chosen.length, "row")} from the library`}
      </Button>
    {/if}
  {/snippet}
</WideModal>

<style>
  h3 {
    margin: 12px 0 2px;
    font-size: 13px;
  }
  h3:first-child {
    margin-top: 0;
  }
  .list {
    list-style: none;
    margin: 6px 0 0;
    padding: 0;
    max-height: 200px;
    overflow: auto;
    border: 1px solid var(--line);
    border-radius: 4px;
  }
  li:nth-child(odd) {
    background: var(--row);
  }
  /* thousands of rows stay cheap: off-screen ones are not laid out */
  li {
    content-visibility: auto;
    contain-intrinsic-size: auto 2.6em;
  }
  label,
  li.row {
    display: grid;
    gap: 0 8px;
    padding: 4px 8px;
  }
  label {
    grid-template-columns: auto 1fr;
    cursor: pointer;
  }
  li.row {
    grid-template-columns: 1fr;
  }
  .loc {
    font-size: 11px;
    grid-column: 2;
    overflow-wrap: anywhere;
  }
  li.row .loc {
    grid-column: 1;
  }
  .name {
    overflow-wrap: anywhere;
  }
  .ack {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-top: 8px;
    cursor: pointer;
  }
  .err {
    color: var(--bad-text);
    background: var(--bad-bg);
    border: 1px solid var(--bad);
    border-radius: 4px;
    padding: 6px 8px;
    overflow-wrap: anywhere;
  }
</style>
