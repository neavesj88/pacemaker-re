<script lang="ts">
  import { untrack } from "svelte";
  import { mmss } from "$lib/format";
  import { nav } from "$lib/state/nav.svelte";
  import { selection } from "$lib/state/selection.svelte";
  import { session } from "$lib/state/session.svelte";
  import { stems } from "$lib/state/stems.svelte";
  import Button from "$lib/ui/Button.svelte";
  import { formatEta, looksLikeDevice, noun, phaseText, runFraction } from "./logic";
  import PlanModal from "./PlanModal.svelte";

  const lib = $derived(session.library);
  let q = $state("");
  let reviewing = $state(false);

  // a deep link from another view: nav.go("stems", { "stems.trackIds": [..] })
  $effect(() => {
    const ids = nav.take<number[]>("stems.trackIds");
    if (ids?.length) untrack(() => stems.setTracks(ids));
  });

  const shown = $derived.by(() => {
    const t = q.trim().toLowerCase();
    const all = lib?.tracks ?? [];
    return t ? all.filter((x) => `${x.artist} ${x.title}`.toLowerCase().includes(t)) : all;
  });
  const chosen = $derived(new Set(stems.trackIds));
  const allShownChosen = $derived(shown.length > 0 && shown.every((t) => chosen.has(t.track_id)));
  const sel = $derived(selection.trackIds.filter((id) => lib?.tracks.some((t) => t.track_id === id)));

  const onDevice = $derived(looksLikeDevice(stems.dest, lib?.root));
  const notReady = $derived(stems.status ? stems.status.blocked : "Reading the status…");
  const canReview = $derived(!!lib && stems.trackIds.length > 0 && stems.dest.trim().length > 0 && !onDevice && !stems.busy && !notReady && !stems.planning);

  function toggleShown() {
    const ids = shown.map((t) => t.track_id);
    stems.setTracks(allShownChosen ? stems.trackIds.filter((id) => !ids.includes(id)) : [...stems.trackIds, ...ids]);
  }

  async function review() {
    reviewing = await stems.review();
  }

  const p = $derived(stems.progress);
  const frac = $derived(runFraction(p));
  const eta = $derived(formatEta(p?.eta_secs));
</script>

<section class="card">
  <header>
    <h4>Make stems</h4>
    <span class="muted">{noun(stems.trackIds.length, "track")} chosen</span>
  </header>

  {#if !lib}
    <p class="muted">Open a Pacemaker drive to choose tracks.</p>
  {:else}
    <div class="tools">
      <input class="search" type="search" placeholder="Search artist or title" bind:value={q} aria-label="Search tracks" disabled={stems.running} />
      <Button small disabled={stems.running || shown.length === 0} onclick={toggleShown}>{allShownChosen ? "Clear shown" : `Choose ${shown.length} shown`}</Button>
      <Button small disabled={stems.running || sel.length === 0} title="The tracks highlighted in the Tracks view" onclick={() => stems.setTracks(sel)}>
        Use the {sel.length} selected in Tracks
      </Button>
      <Button small disabled={stems.running || stems.trackIds.length === 0} onclick={() => stems.setTracks([])}>Clear</Button>
    </div>

    <ul class="list" aria-label="Tracks">
      {#each shown as t (t.track_id)}
        <li>
          <label class:on={chosen.has(t.track_id)}>
            <input type="checkbox" checked={chosen.has(t.track_id)} disabled={stems.running} onchange={() => stems.toggle(t.track_id)} />
            <span class="name">{t.artist || "Unknown artist"} - {t.title || "Untitled"}</span>
            <span class="muted num">{mmss(t.play_time_secs)}</span>
          </label>
        </li>
      {:else}
        <li class="empty">{lib.tracks.length === 0 ? "This library has no tracks." : "No track matches the search."}</li>
      {/each}
    </ul>

    <div class="dest">
      <label for="stems-dest">Save the files in</label>
      <div class="inline">
        <input id="stems-dest" placeholder="A folder on this computer" value={stems.dest} oninput={(e) => stems.setDest(e.currentTarget.value)} disabled={stems.running} />
        <Button small disabled={stems.running} onclick={() => stems.chooseDest()}>Choose…</Button>
      </div>
    </div>
    {#if onDevice}
      <p class="warn" role="alert">That folder is on the Pacemaker. Choose a folder on this computer; adding the results to the card is a separate step afterwards, and asks first.</p>
    {/if}
    {#if stems.planError}<p class="warn" role="alert">{stems.planError}</p>{/if}

    {#if stems.running}
      <div class="run" role="status">
        <div class="bar"><i style="width:{Math.round(frac * 100)}%"></i></div>
        <div class="line">
          <span class="name">{p?.label || "Starting…"}</span>
          <span class="num muted">{Math.round(frac * 100)}%{eta ? `, ${eta} left` : ""}</span>
        </div>
        <div class="line muted">
          <span>{phaseText(p)}</span>
          <span>{p?.device ?? ""}</span>
        </div>
        <div class="stop">
          <Button small variant="ghost" disabled={stems.cancelling} onclick={() => stems.cancel()}>{stems.cancelling ? "Stopping after this part…" : "Cancel"}</Button>
        </div>
      </div>
    {:else}
      <div class="go">
        <span class="muted why">{notReady ?? (stems.busy ? "Another job is running." : "")}</span>
        <Button variant="primary" disabled={!canReview} onclick={review}>{stems.planning ? "Checking…" : "Review what will happen…"}</Button>
      </div>
    {/if}
  {/if}
</section>

{#if reviewing && stems.plan}
  <PlanModal onclose={() => (reviewing = false)} />
{/if}

<style>
  .card {
    background: var(--panel);
    border: 1px solid var(--line);
    border-radius: 8px;
    padding: 12px;
    min-width: 0;
  }
  header {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    margin-bottom: 8px;
  }
  h4 {
    margin: 0;
    font-size: 13px;
  }
  .tools {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    align-items: center;
  }
  .search {
    flex: 1;
    min-width: 160px;
  }
  .list {
    list-style: none;
    margin: 8px 0;
    padding: 0;
    max-height: 280px;
    overflow: auto;
    border: 1px solid var(--line);
    border-radius: 4px;
    background: var(--deep);
  }
  li {
    content-visibility: auto;
    contain-intrinsic-size: auto 28px;
  }
  li label {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 4px 10px;
    cursor: pointer;
  }
  li label:hover {
    background: var(--hover);
  }
  li label.on {
    background: var(--accent-soft);
  }
  .name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .dest label {
    display: block;
    font-size: 12px;
    margin-bottom: 3px;
  }
  .inline {
    display: flex;
    gap: 8px;
    align-items: center;
  }
  .inline input {
    flex: 1;
    min-width: 0;
  }
  .go {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 12px;
    margin-top: 12px;
  }
  .why {
    font-size: 12px;
  }
  .warn {
    margin: 8px 0 0;
    padding: 6px 10px;
    border-radius: 4px;
    background: var(--bad-bg);
    color: var(--bad-text);
    font-size: 12px;
  }
  .run {
    margin-top: 12px;
  }
  .bar {
    height: 8px;
    border-radius: 4px;
    background: var(--row-alt);
    overflow: hidden;
  }
  .bar i {
    display: block;
    height: 100%;
    background: var(--accent-ramp);
    transition: width 0.3s;
  }
  .line {
    display: flex;
    justify-content: space-between;
    gap: 10px;
    margin-top: 4px;
    font-size: 12px;
  }
  .stop {
    text-align: right;
    margin-top: 4px;
  }
</style>
