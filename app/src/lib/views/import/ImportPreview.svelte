<script lang="ts">
  // The preview of an import from rekordbox / iTunes / Traktor / Serato / an M3U playlist: every
  // rating change, every case that would be created or extended, the cue points (written only in
  // the mode the user picks) and the files that found no match. Nothing is written from here;
  // "Apply" opens the confirmation that names every change again.
  import type { CaseAction, CueAction, CueMode } from "$lib/api/import";
  import { jobs } from "$lib/state/jobs.svelte";
  import { session } from "$lib/state/session.svelte";
  import { LABEL_COLOURS, colourName, isColour } from "$lib/state/labels";
  import Button from "$lib/ui/Button.svelte";
  import ColourSwatch from "../tracks/ColourSwatch.svelte";
  import { summary as colourSummary, usedIndexes } from "./colourImport";
  import { cueApplies, cueTime } from "./importLogic";
  import { interop } from "./interop.svelte";

  const ACTION: Record<CaseAction, string> = {
    create: "New case",
    extend: "Add to existing case",
    unchanged: "Already complete",
    skip: "No tracks found here",
  };
  const CUE_MODES: [CueMode, string][] = [
    ["off", "Leave cue points alone"],
    ["empty", "Set cue points where the track has none"],
    ["overwrite", "Set cue points, replacing existing ones"],
  ];
  const CUE_STATE: Record<CueAction, string> = { set: "no cue yet", replace: "has a different cue", same: "already this cue" };
  const stars = (n: number) => (n > 0 ? `${n}★` : "unrated");
  const who = (title: string, artist: string) => (title || "(untitled)") + (artist ? ` · ${artist}` : "");

  let plan = $derived(interop.plan);
  /** Colour labels the ticked option would set but that the Apply button below does not cover. */
  let pendingColours = $derived(interop.colourLabels ? (interop.colourPlan?.changes.length ?? 0) : 0);
</script>

{#if plan}
  <section class="preview card">
    <header>
      <h3>Preview: {interop.label} · {interop.fileName}</h3>
      <Button small variant="ghost" onclick={() => interop.close()}>Close</Button>
    </header>
    <p class="summary">
      {plan.foreign_tracks} tracks in the file; <b>{plan.matched}</b> found {session.place.on}
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
                {#if c.unmatched}{doable || c.already_in ? " · " : ""}{c.unmatched} not {session.place.on}{/if}
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

    {#if plan.cues.length}
      <div class="cues">
        <h4>Cue points ({plan.cues.length})</h4>
        <label>
          <select bind:value={interop.cueMode} aria-label="What to do with cue points">
            {#each CUE_MODES as [v, label] (v)}<option value={v}>{label}</option>{/each}
          </select>
        </label>
        <p class="muted hint">
          The device keeps one cue point per track, and this writes it in the same unit as the beat grid editor does (the
          unit is inferred, not yet checked on a device). Only the first plain cue of each track is considered. Applied
          cues show on the device, not as extra markers.
        </p>
        <details open={plan.cues.length <= 8}>
          <summary>{interop.cueSteps.length} of {plan.cues.length} would be written</summary>
          <ul class="rows">
            {#each plan.cues as c (c.track_id)}
              <li>
                <span class="t">{who(c.title, c.artist)}</span>
                <span class="num">
                  {cueTime(c.cue_secs)} · {CUE_STATE[c.action]} ·
                  <b>{cueApplies(interop.cueMode, c.action) ? "will be set" : "left alone"}</b>
                </span>
              </li>
            {/each}
          </ul>
        </details>
      </div>
    {/if}

    {#if plan.colours.length}
      {@const cp = interop.colourPlan}
      <div class="colours">
        <h4>Track colours ({plan.colours.length})</h4>
        <label><input type="checkbox" bind:checked={interop.colourLabels} /> Also set colour labels from the colours {interop.label} gave these tracks (a separate step with its own button, not part of Apply below)</label>
        <p class="muted hint">
          Colour labels belong to this app (Tracks, Tags, colour, energy); the Pacemaker has no track colours, so nothing is
          written to the device. The file stores a colour number, not a name, and what {interop.label} calls each number is not
          verified here: the table below starts with every number set to "No colour", so nothing is set until you choose. "Fill in palette order" is a guess; check it against the colours you see in {interop.label}.
        </p>
        {#if interop.colourLabels && cp}
          <table class="map" aria-label="Colour numbers in the file and the label colour each becomes">
            <thead><tr><th>Colour number in the file</th><th>Tracks</th><th>Becomes the label colour</th></tr></thead>
            <tbody>
              {#each usedIndexes(plan.colours) as i (i)}
                {@const to = interop.colourMap[i] ?? null}
                <tr>
                  <td class="num">{i}</td>
                  <td class="num">{plan.colours.filter((c) => c.index === i).length}</td>
                  <td>
                    <ColourSwatch colour={to} size={12} empty />
                    <select value={to ?? ""} aria-label="Label colour for colour number {i}" onchange={(e) => interop.setColour(i, isColour(e.currentTarget.value) ? e.currentTarget.value : null)}>
                      <option value="">No colour (leave these tracks alone)</option>
                      {#each LABEL_COLOURS as c (c)}<option value={c}>{colourName(c)}</option>{/each}
                    </select>
                  </td>
                </tr>
              {/each}
            </tbody>
          </table>
          <Button onclick={() => interop.useSuggestedColours()}>Fill in palette order (a guess)</Button>
          <label class="repl"><input type="checkbox" bind:checked={interop.replaceColours} /> Replace a colour label a track already has</label>
          <details open={cp.changes.length > 0 && cp.changes.length <= 10}>
            <summary>{cp.changes.length} colour label{cp.changes.length === 1 ? "" : "s"} would be set</summary>
            {#if cp.changes.length === 0}
              <p class="muted">Nothing to set with this table.</p>
            {:else}
              <ul class="rows">
                {#each cp.changes as c (c.track_id)}
                  <li><span class="t">{who(c.title, c.artist)}</span><span class="num">{c.from ? colourName(c.from) : "no colour"} → <b>{colourName(c.to)}</b></span></li>
                {/each}
              </ul>
            {/if}
          </details>
          <p class="muted hint">{colourSummary(cp)}</p>
          {#if interop.askingColours}
            <div class="ask" role="alertdialog" aria-label="Set colour labels">
              <p>
                Set the colour label of the {cp.changes.length} track{cp.changes.length === 1 ? "" : "s"} listed above?
                {#if cp.replaced > 0}{cp.replaced} of them already {cp.replaced === 1 ? "has" : "have"} a colour that will be replaced (shown in the list).{/if}
                You can change any label again in Tracks.
              </p>
              <Button data-autofocus onclick={() => (interop.askingColours = false)}>Cancel</Button>
              <Button variant="primary" disabled={interop.applyingColours} onclick={() => interop.applyColours()}>{interop.applyingColours ? "Setting…" : `Set ${cp.changes.length} colour label${cp.changes.length === 1 ? "" : "s"}`}</Button>
            </div>
          {:else}
            <Button disabled={cp.changes.length === 0 || interop.loading} onclick={() => (interop.askingColours = true)}>Set {cp.changes.length} colour label{cp.changes.length === 1 ? "" : "s"}…</Button>
          {/if}
        {/if}
      </div>
    {/if}

    {#if plan.notes.length}
      <ul class="notes" aria-label="Notes from reading the file">
        {#each plan.notes as n, i (i)}<li>{n}</li>{/each}
      </ul>
    {/if}

    {#if plan.unmatched_total}
      <details>
        <summary>Not found {session.place.on} ({plan.unmatched_total})</summary>
        <ul class="rows">
          {#each plan.unmatched as u, i (i)}
            <li>
              <span class="t">{who(u.title, u.artist)}{#if u.in_playlist} <span class="badge create">in a playlist</span>{/if}</span>
              <span class="muted why">{u.reason}{u.path ? ` · ${u.path_checked ? (u.on_disk ? "file found" : "file not found") : "location"}: ${u.path}` : ""}</span>
            </li>
          {/each}
          {#if plan.unmatched_total > plan.unmatched.length}
            <li class="muted">…and {plan.unmatched_total - plan.unmatched.length} more (only the first {plan.unmatched.length} are listed; the counts and the add button cover all of them)</li>
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
      <Button variant="primary" disabled={!jobs.canWrite || interop.loading || interop.changes === 0} onclick={() => interop.ask()}>
        {interop.changes === 0 ? (pendingColours > 0 ? "No ratings, cues or cases to apply" : "Nothing to apply") : `Apply ${interop.changes} change${interop.changes === 1 ? "" : "s"}…`}
      </Button>
    </footer>
    {#if pendingColours > 0}
      <p class="muted hint" role="status">
        {pendingColours} colour label{pendingColours === 1 ? " is" : "s are"} not set yet: use the "Set {pendingColours} colour label{pendingColours === 1 ? "" : "s"}…" button in the Track colours section.
        Apply only writes ratings, cue points and cases.
      </p>
    {/if}
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
  .colours select,
  .cues select {
    background: var(--panel);
    color: var(--text);
    border: 1px solid var(--line);
    border-radius: 4px;
    padding: 5px 8px;
    font: inherit;
    max-width: 100%;
  }
  .map {
    border-collapse: collapse;
    margin: 6px 0;
  }
  .map th {
    text-align: left;
    font-weight: 600;
    font-size: 12px;
    color: var(--muted);
    padding: 2px 14px 2px 0;
  }
  .map td {
    padding: 2px 14px 2px 0;
  }
  .map td:last-child {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .repl {
    display: block;
    margin: 4px 0;
  }
  .ask {
    margin-top: 8px;
    padding: 8px 10px;
    background: var(--row);
    border: 1px solid var(--line-hi);
    border-radius: 6px;
  }
  .ask p {
    margin: 0 0 8px;
  }
  .notes {
    margin: 8px 0;
    padding: 6px 12px 6px 28px;
    border-radius: 6px;
    background: var(--row);
    border: 1px solid var(--line);
    overflow-wrap: anywhere;
  }
  .hint {
    margin: 6px 0 0;
    font-size: 12px;
  }
</style>
