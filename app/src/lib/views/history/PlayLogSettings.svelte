<script lang="ts">
  import type { PlayLogSettings } from "$lib/api/history";
  import { playlog } from "$lib/state/playlog.svelte";
  import Button from "$lib/ui/Button.svelte";
  import { retentionLabel } from "./playlogLogic";

  // The play log's choices. Each one saves as soon as it changes; shortening the retention asks
  // first because the individual dates of the older plays are folded away.
  const GAPS = [
    { min: 60, label: "1 hour" },
    { min: 120, label: "2 hours" },
    { min: 240, label: "4 hours" },
    { min: 480, label: "8 hours" },
    { min: 720, label: "12 hours" },
  ];

  let err = $state("");
  let busy = $state(false);
  let retention = $state("");
  // A shorter retention waiting for the user's yes.
  let pendingDays = $state<number | null>(null);

  $effect(() => {
    retention = String(playlog.settings.retention_days);
  });

  async function commit(patch: Partial<PlayLogSettings>) {
    err = "";
    busy = true;
    try {
      await playlog.save({ ...playlog.settings, ...patch });
    } catch (e) {
      err = String(e);
    } finally {
      busy = false;
    }
  }

  function commitRetention() {
    const n = Number(retention);
    if (!Number.isInteger(n) || n < 0 || n > 36500) {
      err = "Days kept must be a whole number from 0 to 36500 (0 keeps everything).";
      retention = String(playlog.settings.retention_days);
      return;
    }
    err = "";
    const now = playlog.settings.retention_days;
    if (n === now) return;
    const shorter = n > 0 && (now === 0 || n < now);
    if (shorter) pendingDays = n;
    else void commit({ retention_days: n });
  }

  async function confirmShorter() {
    const n = pendingDays;
    pendingDays = null;
    if (n !== null) await commit({ retention_days: n });
  }
  function cancelShorter() {
    pendingDays = null;
    retention = String(playlog.settings.retention_days);
  }
</script>

<div class="panel">
  <label class="check">
    <input type="checkbox" checked={playlog.settings.enabled} disabled={busy} onchange={(e) => commit({ enabled: e.currentTarget.checked })} />
    Record the play counters each time a library is opened
  </label>
  <p class="hint">
    On by default. Only changes are stored, in a file on this computer (not on the Pacemaker). Turning it off keeps the log you have; it
    just stops growing.
  </p>

  <label class="field">
    Days of changes to keep
    <input
      type="text"
      inputmode="numeric"
      bind:value={retention}
      disabled={busy || pendingDays !== null}
      onchange={commitRetention}
      onkeydown={(e) => e.key === "Enter" && commitRetention()}
    />
    <span class="muted">{retentionLabel(playlog.settings.retention_days)}; 0 keeps everything</span>
  </label>
  <p class="hint">
    Older changes are folded into the starting point: their play counts are kept but their individual dates are not. Applied the next time a
    library is opened.
  </p>
  {#if pendingDays !== null}
    <div class="ask" role="alertdialog" aria-label="Confirm a shorter retention">
      Keep only {pendingDays} day{pendingDays === 1 ? "" : "s"}? Plays older than that lose their dates (their counts stay in the total) and this cannot be undone.
      <span class="btns">
        <Button small data-autofocus onclick={cancelShorter}>Cancel</Button>
        <Button small variant="primary" onclick={confirmShorter}>Keep {pendingDays} days</Button>
      </span>
    </div>
  {/if}

  <label class="field">
    A pause ends a session after
    <select value={playlog.settings.session_gap_minutes} disabled={busy} onchange={(e) => commit({ session_gap_minutes: Number(e.currentTarget.value) })}>
      {#each GAPS as g (g.min)}<option value={g.min}>{g.label}</option>{/each}
      {#if !GAPS.some((g) => g.min === playlog.settings.session_gap_minutes)}
        <option value={playlog.settings.session_gap_minutes}>{playlog.settings.session_gap_minutes} minutes</option>
      {/if}
    </select>
  </label>
  {#if err}<p class="err" role="alert">{err}</p>{/if}
</div>

<style>
  .panel {
    margin: 0 12px 10px;
    padding: 10px 12px;
    border: 1px solid var(--line-hi);
    border-radius: 6px;
    background: var(--panel);
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .check,
  .field {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
  }
  .field input,
  .field select {
    background: var(--deep);
    color: var(--text);
    border: 1px solid var(--line-hi);
    border-radius: 4px;
    padding: 3px 6px;
    font: inherit;
  }
  .field input {
    width: 90px;
  }
  .hint {
    margin: 0 0 6px;
    color: var(--muted);
    font-size: 12px;
  }
  .ask {
    border: 1px solid var(--line-hi);
    border-left: 3px solid var(--accent);
    border-radius: 6px;
    padding: 8px 10px;
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 10px;
  }
  .btns {
    display: inline-flex;
    gap: 6px;
  }
  .err {
    margin: 0;
    color: var(--bad);
  }
</style>
