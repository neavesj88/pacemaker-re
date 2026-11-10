<script lang="ts">
  // The preview of an import from rekordbox / iTunes: every rating change, every case that would
  // be created or extended, the cues left alone and the files that found no match. Nothing is
  // written from here; "Apply" opens the confirmation that names every change again.
  import type { CaseAction } from "$lib/api/import";
  import { mmss } from "$lib/format";
  import { jobs } from "$lib/state/jobs.svelte";
  import Button from "$lib/ui/Button.svelte";
  import { interop } from "./interop.svelte";

  const ACTION: Record<CaseAction, string> = {
    create: "New case",
    extend: "Add to existing case",
    unchanged: "Already complete",
    skip: "No tracks on the device",
  };
  const stars = (n: number) => (n > 0 ? `${n}★` : "unrated");
  const who = (title: string, artist: string) => (title || "(untitled)") + (artist ? ` · ${artist}` : "");

  let plan = $derived(interop.plan);
</script>

{#if plan}
  <section class="preview card">
    <header>
      <h3>Preview: {interop.label} · {interop.fileName}</h3>
      <Button small variant="ghost" onclick={() => interop.close()}>Close</Button>
    </header>
    <p class="summary">
      {plan.foreign_tracks} tracks in the file; <b>{plan.matched}</b> found on the device
      <span class="muted">({plan.matched_by_file} by file name and size, {plan.matched_by_tags} by title and artist)</span>;
      <b>{plan.unmatched_total}</b> not found. Nothing is written until you apply.
    </p>

    <div class="opts">
      <label><input type="checkbox" bind:checked={interop.ratings} /> Ratings ({plan.ratings.length} to change, {plan.ratings_unchanged} already the same)</label>
      <label><input type="checkbox" bind:checked={interop.playlists} /> Playlists as cases ({plan.cases.length} in the file)</label>
    </div>

    {#if interop.ratings}
      <details open={plan.ratings.length > 0 && plan.ratings.length <= 12}>
        <summary>Ratings that change ({plan.ratings.length})</summary>
        {#if plan.ratings.length === 0}
          <p class="muted">Every matched track already has the rating the file gives it (unrated tracks in the file never clear a rating).</p>
        {:else}
          <ul class="rows">
            {#each plan.ratings as r (r.track_id)}
              <li><span class="t">{who(r.title, r.artist)}</span><span class="num">{stars(r.from)} → <b>{stars(r.to)}</b></span></li>
            {/each}
          </ul>
        {/if}
      </details>
    {/if}

    {#if interop.playlists}
      <div class="cases">
        <h4>Cases</h4>
        {#each plan.cases as c (c.index)}
          {@const doable = c.action === "create" || c.action === "extend"}
          <details class="case" class:off={!doable}>
            <summary>
              <input
                type="checkbox"
                disabled={!doable}
                checked={doable && interop.chosen.includes(c.index)}
                onclick={(e) => e.stopPropagation()}
                onchange={() => interop.toggleCase(c.index)}
                aria-label={`Include ${c.name}`}
              />
              <span class="name">{c.name}</span>
              <span class="badge {c.action}">{ACTION[c.action]}</span>
              <span class="muted num">
                {#if doable}+{c.tracks.length}{/if}
                {#if c.already_in}{doable ? " · " : ""}{c.already_in} already in{/if}
                {#if c.unmatched}{doable || c.already_in ? " · " : ""}{c.unmatched} not on the device{/if}
              </span>
            </summary>
            {#if c.tracks.length}
              <ol class="rows">
                {#each c.tracks as t (t.track_id)}<li><span class="t">{who(t.title, t.artist)}</span></li>{/each}
              </ol>
            {:else}
              <p class="muted">Nothing to add.</p>
            {/if}
          </details>
        {/each}
        {#if plan.cases.length === 0}<p class="muted">The file has no playlists.</p>{/if}
      </div>
    {/if}

    {#if plan.cues_skipped.length}
      <details>
        <summary>Memory cues left alone ({plan.cues_skipped.length})</summary>
        <p class="muted">
          The device keeps a single cue point per track and this version has no operation to set it, so these cues are
          reported, not imported.
        </p>
        <ul class="rows">
          {#each plan.cues_skipped as c (c.track_id)}
            <li><span class="t">{who(c.title, c.artist)}</span><span class="num">first memory cue at {mmss(Math.round(c.cue_secs))}</span></li>
          {/each}
        </ul>
      </details>
    {/if}

    {#if plan.unmatched_total}
      <details>
        <summary>Not found on the device ({plan.unmatched_total})</summary>
        <ul class="rows">
          {#each plan.unmatched as u, i (i)}
            <li>
              <span class="t">{who(u.title, u.artist)}{#if u.in_playlist} <span class="badge create">in a playlist</span>{/if}</span>
              <span class="muted why">{u.reason}{u.path ? ` · ${u.on_disk ? "file found" : "file not found"}: ${u.path}` : ""}</span>
            </li>
          {/each}
          {#if plan.unmatched_total > plan.unmatched.length}
            <li class="muted">…and {plan.unmatched_total - plan.unmatched.length} more</li>
          {/if}
        </ul>
      </details>
    {/if}

    <footer>
      {#if plan.addable_paths.length}
        <Button disabled={!jobs.canWrite} onclick={() => jobs.askAdd(plan.addable_paths)}>
          Add {plan.addable_paths.length} unmatched file{plan.addable_paths.length === 1 ? "" : "s"} from this computer…
        </Button>
      {/if}
      <span class="spacer"></span>
      {#if interop.loading}<span class="muted">Reading…</span>{/if}
      <Button variant="primary" disabled={!jobs.canWrite || interop.changes === 0} onclick={() => interop.ask()}>
        {interop.changes === 0 ? "Nothing to apply" : `Apply ${interop.changes} change${interop.changes === 1 ? "" : "s"}…`}
      </Button>
    </footer>
    {#if plan.addable_paths.length}
      <p class="muted hint">Adding files first lets more playlist entries match: the preview re-reads when the add finishes.</p>
    {/if}
  </section>
{/if}

<style>
  .preview {
    grid-column: 1 / -1;
    padding: 12px 14px;
    background: var(--panel);
    border: 1px solid var(--line);
    border-radius: 8px;
  }
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }
  h3,
  h4 {
    margin: 0 0 6px;
    font-size: 14px;
  }
  h4 {
    margin-top: 10px;
  }
  .summary {
    margin: 0 0 8px;
  }
  .opts {
    display: flex;
    flex-wrap: wrap;
    gap: 6px 18px;
    margin-bottom: 8px;
  }
  details {
    margin: 4px 0;
  }
  summary {
    cursor: pointer;
  }
  .case summary {
    display: flex;
    align-items: baseline;
    flex-wrap: wrap;
    gap: 4px 10px;
    padding: 3px 0;
  }
  .case.off {
    opacity: 0.7;
  }
  .name {
    font-weight: 600;
  }
  .rows {
    margin: 4px 0 6px;
    padding-left: 18px;
    max-height: 220px;
    overflow: auto;
  }
  .rows li {
    display: flex;
    justify-content: space-between;
    gap: 12px;
    padding: 1px 0;
  }
  .t {
    min-width: 0;
    overflow-wrap: anywhere;
  }
  .why {
    text-align: right;
    overflow-wrap: anywhere;
    font-size: 12px;
  }
  .badge {
    font-size: 11px;
    padding: 1px 7px;
    border-radius: 9px;
    border: 1px solid var(--line-hi);
    color: var(--muted);
  }
  .badge.create,
  .badge.extend {
    color: var(--green);
    border-color: var(--green);
  }
  footer {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 8px;
    margin-top: 10px;
  }
  .spacer {
    flex: 1;
  }
  .hint {
    margin: 6px 0 0;
    font-size: 12px;
  }
</style>
