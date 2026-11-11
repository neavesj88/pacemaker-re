<script lang="ts">
  // Duplicates: find tracks that are the same recording, look at every difference between the
  // copies, choose the one to keep and merge the rest into it. Finding changes nothing. A merge
  // is reviewed in a dialog that names every change, and removing copies from the library and
  // deleting their files are separate choices made there.
  import { onMount } from "svelte";
  import { plural, waysLine } from "$lib/state/dupeLogic";
  import { duplicates as d } from "$lib/state/duplicates.svelte";
  import { jobs } from "$lib/state/jobs.svelte";
  import { session } from "$lib/state/session.svelte";
  import Button from "$lib/ui/Button.svelte";
  import DupGroupCard from "./DupGroupCard.svelte";
  import DupSettingsDialog from "./DupSettingsDialog.svelte";
  import MergeDialog from "./MergeDialog.svelte";

  onMount(() => {
    void d.ensureSettings();
  });

  const s = $derived(d.settings);
  const result = $derived(d.result);
  const noWays = $derived(!s.by_tags && !s.by_size && !s.by_audio);
  const fp = $derived(d.fp);
  const pending = $derived(s.by_audio && result ? result.pending.length : 0);
  const groups = $derived(result?.groups ?? []);
  const pct = $derived(fp && fp.total > 0 ? Math.min(100, Math.round((fp.done / fp.total) * 100)) : 0);

  function toggle(key: "by_tags" | "by_size" | "by_audio", on: boolean) {
    void d.saveSettings({ ...d.settings, [key]: on });
  }
</script>

<div class="page">
  <header class="bar">
    <h2>Duplicates</h2>
    <fieldset class="ways" aria-label="Ways of finding copies">
      <label title="Same artist and title (ignoring case, punctuation and “feat.”), and about the same length"
        ><input type="checkbox" checked={s.by_tags} onchange={(e) => toggle("by_tags", e.currentTarget.checked)} /> Artist and title</label
      >
      <label title="The same file size and length"
        ><input type="checkbox" checked={s.by_size} onchange={(e) => toggle("by_size", e.currentTarget.checked)} /> File size and length</label
      >
      <label title="Compares how the tracks sound. Reads each candidate file once and remembers the result."
        ><input type="checkbox" checked={s.by_audio} onchange={(e) => toggle("by_audio", e.currentTarget.checked)} /> Sound</label
      >
    </fieldset>
    <span class="grow"></span>
    <Button onclick={() => (d.settingsOpen = true)}>Settings…</Button>
    <Button variant="primary" disabled={!session.library || d.finding || noWays} onclick={() => d.find()}>{d.finding ? "Looking…" : result ? "Find again" : "Find duplicates"}</Button>
  </header>

  <p class="muted line">
    {waysLine(s)}. Finding changes nothing; you choose what to merge and confirm it first.
    {#if d.stale}<b class="stale"> The ways changed since the last search: find again to update the sets.</b>{/if}
  </p>

  {#if d.settingsWarning}<p class="warn" role="alert">{d.settingsWarning}</p>{/if}
  {#if d.error}<p class="warn" role="alert">{d.error}</p>{/if}
  {#if result}
    {#each result.warnings as w, i (i)}<p class="muted line">{w}</p>{/each}
  {/if}

  {#if fp}
    <div class="panel" role="status">
      {#if fp.running}
        <div class="row">
          <b>Fingerprinting {fp.done} / {fp.total}</b>
          <span class="cur muted">{fp.current}</span>
          <Button small disabled={fp.cancelling} onclick={() => d.cancelFingerprinting()}>{fp.cancelling ? "Stopping…" : "Cancel"}</Button>
        </div>
        <progress max={fp.total || 1} value={fp.done}></progress>
        <p class="muted small">{pct}% · each file is read once; the result is kept, so a second search is quick. Cancel keeps what is done.</p>
      {:else}
        <div class="row">
          <b>Fingerprinting finished:</b>
          <span class="cur">{fp.summary}</span>
          <Button small onclick={() => d.dismissFingerprint()}>Dismiss</Button>
        </div>
      {/if}
      {#if fp.failures.length > 0}
        <ul class="fails">
          {#each fp.failures as f, i (i)}<li><b>{f.label}</b>: {f.error}</li>{/each}
        </ul>
      {/if}
    </div>
  {:else if pending > 0}
    <div class="panel" role="status">
      <div class="row">
        <span>
          The sound check needs <b>{plural(pending, "track")}</b> fingerprinted first. Only tracks with another track of about the same length are read.
        </span>
        <span class="grow"></span>
        <Button variant="primary" disabled={jobs.busy} onclick={() => d.startFingerprinting()}>Fingerprint {plural(pending, "track")}</Button>
      </div>
    </div>
  {:else if s.by_audio && result}
    <p class="muted line">Sound check: {plural(result.fingerprinted, "track")} compared.</p>
  {/if}

  {#if !session.library}
    <p class="empty">Open a library to look for duplicates.</p>
  {:else if !result && !d.finding}
    <p class="empty">
      Press <b>Find duplicates</b>. Sets of copies appear here, each showing every field that differs, so you can pick the one to keep.
    </p>
  {:else if result && groups.length === 0}
    <p class="empty">
      No copies found among {plural(result.tracks, "track")}{noWays ? ", because no way of finding them is switched on" : ""}.
      {#if pending > 0}The sound check has not run on {plural(pending, "track")} yet.{/if}
    </p>
  {:else if result}
    <div class="summary">
      <b>{plural(groups.length, "set")} of copies</b>
      <span class="muted">among {plural(result.tracks, "track")}</span>
      <span class="grow"></span>
      <Button small variant="link" onclick={() => d.pickAll(true)}>Tick all</Button>
      <Button small variant="link" onclick={() => d.pickAll(false)}>Tick none</Button>
    </div>
    <div class="groups">
      {#each groups as g (g.id)}
        {#if d.choices[g.id]}<DupGroupCard group={g} choice={d.choices[g.id]} />{/if}
      {/each}
    </div>
  {/if}

  {#if result && groups.length > 0}
    <footer class="apply">
      <span>{plural(d.picked, "set")} ticked to merge</span>
      <span class="muted small">Nothing is changed until you confirm the review.</span>
      <span class="grow"></span>
      <Button variant="primary" disabled={d.picked === 0 || !jobs.canWrite} onclick={() => d.openMerge()}>Review merge…</Button>
    </footer>
  {/if}
</div>

{#if d.settingsOpen}<DupSettingsDialog />{/if}
<MergeDialog />

<style>
  .page {
    display: flex;
    flex-direction: column;
    box-sizing: border-box;
    height: 100%;
    flex: 1;
    min-width: 0;
    min-height: 0;
    overflow: auto;
    padding: 14px 18px 0;
  }
  .bar {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 10px 16px;
  }
  h2 {
    margin: 0;
    font-size: 18px;
  }
  .ways {
    display: flex;
    flex-wrap: wrap;
    gap: 4px 16px;
    margin: 0;
    padding: 0;
    border: 0;
  }
  .grow {
    flex: 1;
  }
  .line {
    margin: 6px 0;
  }
  .stale {
    color: var(--accent);
  }
  .warn {
    margin: 6px 0;
    color: var(--bad-text);
  }
  .small {
    font-size: 12px;
  }
  .panel {
    margin: 8px 0;
    padding: 8px 12px;
    background: var(--panel);
    border: 1px solid var(--line-hi);
    border-radius: 6px;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .cur {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  progress {
    width: 100%;
    height: 6px;
    margin-top: 6px;
    accent-color: var(--accent);
  }
  .panel p {
    margin: 4px 0 0;
  }
  .fails {
    margin: 6px 0 0;
    padding-left: 18px;
    max-height: 90px;
    overflow: auto;
    color: var(--bad-text);
  }
  .summary {
    display: flex;
    align-items: baseline;
    gap: 10px;
    margin: 10px 0 6px;
  }
  .groups {
    padding-bottom: 8px;
  }
  .empty {
    margin: 40px auto;
    max-width: 52ch;
    text-align: center;
    color: var(--muted);
  }
  .apply {
    position: sticky;
    bottom: 0;
    display: flex;
    align-items: center;
    gap: 14px;
    margin: 0 -18px;
    padding: 10px 18px;
    background: var(--panel);
    border-top: 1px solid var(--line-hi);
  }
</style>
