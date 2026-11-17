<script lang="ts">
  import { deviceRestoreSnapshot, deviceSnapshotPreview, type SnapshotDiff, type SnapshotRow } from "$lib/api/device";
  import { session } from "$lib/state/session.svelte";
  import { toast } from "$lib/state/toast.svelte";
  import Button from "$lib/ui/Button.svelte";
  import { device } from "./device.svelte";
  import { plural } from "./format";
  import { diffSentences, LIST_HEADINGS, LIST_ORDER, moreText, stampLabel } from "./trustLogic";
  import WideModal from "./WideModal.svelte";

  // The named confirmation of a restore: which snapshot, what changes in numbers and in names, what
  // does not change, and that the current music.db is saved first. The preview comes from the same
  // comparison the restore repeats; if anything changed in between, the restore refuses.
  let { row, onclose }: { row: SnapshotRow; onclose: () => void } = $props();

  let diff = $state<SnapshotDiff | null>(null);
  let error = $state("");
  let busy = $state(false);
  const root = session.library?.root ?? "";

  $effect(() => {
    let current = true;
    deviceSnapshotPreview(root, row.name).then(
      (d) => current && (diff = d),
      (e) => current && (error = String(e)),
    );
    return () => (current = false);
  });

  const sentences = $derived(diff ? diffSentences(diff.counts, session.library?.kind) : []);
  const same = $derived(!!diff && !diff.current_problem && sentences.length === 0);

  const holds = (c: { tracks: number; cases: number; mixes: number }) =>
    `${plural(c.tracks, "track")}, ${plural(c.cases, "case")}, ${plural(c.mixes, "mix", "mixes")}`;

  async function restore() {
    if (!diff || busy) return;
    busy = true;
    // `row` is the parent's state and is gone once the dialog closes: read it before that
    const { name, stamp } = row;
    try {
      const rep = await deviceRestoreSnapshot(root, name, diff.fingerprint);
      onclose();
      toast.show(`Restored music.db from ${stampLabel(stamp)}.`, "ok", { details: `${holds(rep.counts)}. The previous one is saved as ${rep.safety_copy}.` });
      await session.load(root, true);
      device.snapshotTick++;
      await device.load(root);
    } catch (e) {
      error = String(e);
      busy = false;
    }
  }
</script>

<WideModal title={`Restore music.db from ${stampLabel(row.stamp)}?`} {onclose} width={700}>
  <div class="facts">
    <span class="k">Snapshot</span><span class="mono">{row.name}</span>
    <span class="k">Card</span><span>{root}</span>
    <span class="k">Holds</span>
    <span>
      the snapshot: {row.snapshot ? holds(row.snapshot) : "unknown"}
      {#if diff?.current}
        <br />{session.place.card} now: {holds(diff.current)}
      {/if}
    </span>
  </div>

  {#if error}
    <p class="err" role="alert">{error}</p>
  {:else if !diff}
    <p class="muted">Comparing with {session.place.card}…</p>
  {:else}
    {#if diff.current_problem}
      <p class="err">
        The library cannot be read now ({diff.current_problem}). Restoring replaces it.
      </p>
    {/if}
    {#if same}
      <p class="muted">Same library as {session.place.card} now. Nothing to restore.</p>
    {:else if sentences.length}
      <p><b>What changes</b></p>
      <ul class="what">
        {#each sentences as s (s)}<li>{s}</li>{/each}
      </ul>
      {#each LIST_ORDER as key (key)}
        {@const l = diff[key]}
        {#if l.total > 0}
          <details>
            <summary>{LIST_HEADINGS[key]} ({l.total})</summary>
            <ul class="names">
              {#each l.items as it (it.id)}
                <li>{it.label}{#if it.note}<span class="note"> — {it.note}</span>{/if}</li>
              {/each}
            </ul>
            {#if moreText(l.total, l.items.length)}<p class="muted more">{moreText(l.total, l.items.length)}</p>{/if}
          </details>
        {/if}
      {/each}
    {/if}
    <p class="muted fine">
      Replaces music.db only; bulk beat-grid edits stay. Current music.db is snapshotted first. Edit history is cleared.
    </p>
  {/if}

  {#snippet actions()}
    <Button data-autofocus onclick={onclose}>Cancel</Button>
    <Button variant="primary" disabled={!diff || same || busy || !!error} onclick={restore}>
      {busy ? "Restoring…" : "Restore this snapshot"}
    </Button>
  {/snippet}
</WideModal>

<style>
  .facts {
    display: grid;
    grid-template-columns: 70px 1fr;
    gap: 4px 8px;
    margin: 4px 0 10px;
    overflow-wrap: anywhere;
  }
  .k {
    color: var(--muted);
    font-size: 12px;
  }
  .mono {
    font-family: ui-monospace, Consolas, monospace;
    font-size: 12px;
  }
  .what {
    margin: 4px 0 8px;
    padding-left: 18px;
  }
  details {
    border: 1px solid var(--line);
    border-radius: 4px;
    margin: 4px 0;
    padding: 4px 8px;
  }
  summary {
    cursor: pointer;
  }
  .names {
    margin: 6px 0 2px;
    padding-left: 18px;
    max-height: 160px;
    overflow: auto;
  }
  .note {
    color: var(--accent);
    font-size: 12px;
  }
  .more {
    margin: 2px 0 0;
    font-size: 12px;
  }
  .err {
    color: var(--bad-text);
    background: var(--bad-bg);
    border: 1px solid var(--bad);
    border-radius: 4px;
    padding: 6px 8px;
    overflow-wrap: anywhere;
  }
  .fine {
    font-size: 12px;
    margin-top: 10px;
  }
</style>
