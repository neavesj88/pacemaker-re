<script lang="ts">
  import { onMount } from "svelte";
  import { session } from "$lib/state/session.svelte";
  import Button from "$lib/ui/Button.svelte";
  import { device } from "./device.svelte";
  import { plural } from "./format";
  import { relink } from "./relink.svelte";
  import {
    actionLine,
    confidenceLabel,
    confirmLine,
    freeSpaceProblem,
    kindLabel,
    locationLine,
    matchSummary,
    nonStandardNote,
    scanLine,
    scanPercent,
    sizeText,
    totalsLine,
  } from "./relinkLogic";
  import RelinkOptionsFields from "./RelinkOptionsFields.svelte";
  import WideModal from "./WideModal.svelte";

  // Missing audio: search a folder (on this PC or on the card) for the files of the tracks the
  // Health scan lists, show every match with the evidence for it, and only then relink. Four
  // steps in one dialog: choose the folder, wait for the search, review (tick what to relink),
  // confirm (every ticked track named, with what happens to it).
  const root = $derived(session.library?.root ?? "");
  const s = $derived(relink.settings);
  const plan = $derived(relink.plan);
  const missing = $derived(device.health?.missing_audio.length ?? 0);

  // a file picked by hand counts as a match for its row
  const matched = $derived(relink.rows.filter((r) => !!relink.candidate(r)));
  const sureCount = $derived(matched.filter((r) => relink.candidate(r)?.confidence === "sure").length);
  const tot = $derived(relink.totals);
  const free = $derived(device.overview?.storage.free_bytes);
  const spaceProblem = $derived(freeSpaceProblem(tot.copyBytes, free, session.library?.kind));
  const chosen = $derived(relink.chosen);
  const oddPaths = $derived(nonStandardNote(chosen.map((c) => c.cand), session.library?.kind));
  const pct = $derived(scanPercent(relink.progress));

  onMount(() => {
    void relink.open();
  });

  const confClass = { sure: "sure", likely: "likely", check: "check" } as const;
</script>

<WideModal title="Find missing audio" onclose={() => relink.close()} width={860}>
  {#if relink.error}
    <p class="err" role="alert">{relink.error}</p>
  {/if}

  {#if relink.stage === "choose"}
    <p>
      {plural(missing, "track")} {missing === 1 ? "is" : "are"} listed in the library without an audio file in {session.place.card}.
      Choose a folder that may hold them, on this PC{session.isComputer ? '' : ' or on the card'}. The files are matched by name and size, and, when
      you allow it, by the title, artist and length in their tags (a file whose length cannot be read, such as an MP3
      without a length header, can then only match by its name or be picked by hand). Nothing changes until you have
      looked at the matches and confirmed.
    </p>
    {#if s}
      <div class="folder">
        <input
          type="text"
          class="path"
          aria-label="Folder to search"
          placeholder={session.isComputer ? 'A folder on this PC' : 'A folder on this PC or on the card'}
          bind:value={relink.folder}
          onkeydown={(e) => e.key === "Enter" && relink.search(root)}
        />
        <Button onclick={() => relink.browse()}>Browse…</Button>
        <Button onclick={() => relink.useCardMusic(root)} title={`Looks for files that are ${session.isComputer ? "in the library" : "on the card"} but not where the library expects them`}>
          {session.isComputer ? "The library's Music folder" : "The card's Music folder"}
        </Button>
      </div>
      {#if relink.settingsWarning}<p class="note">{relink.settingsWarning}</p>{/if}

      <details class="opts">
        <summary>Options</summary>
        <RelinkOptionsFields />
      </details>
    {:else if !relink.error}
      <p class="muted">Reading the options…</p>
    {/if}
  {:else if relink.stage === "scanning"}
    <p>Searching {relink.folder}</p>
    <div class="prog" role="status">
      <div class="bar"><i style="width:{pct}%"></i></div>
      <div class="line muted">
        <span>{scanLine(relink.progress)}</span>
        <span class="name">{relink.progress?.label ?? ""}</span>
      </div>
    </div>
  {:else if relink.stage === "review" && plan}
    <p class="head">{matchSummary(plan.total_missing, matched.length, sureCount)}</p>
    <p class="muted small">
      Searched {plan.folder}: {plural(plan.files_seen, "audio file")}{plan.tag_reads > 0 ? `, tags read from ${plan.tag_reads}` : ""}.
    </p>
    {#each plan.warnings as w (w)}<p class="note">{w}</p>{/each}

    {#if matched.length > 0}
      <div class="bulk">
        <Button small onclick={() => relink.tickWhere("sure")}>Tick the sure ones</Button>
        <Button small onclick={() => relink.tickWhere("likely")}>Tick the sure and likely ones</Button>
        <Button small variant="ghost" onclick={() => relink.tickWhere("none")}>Untick all</Button>
      </div>
    {/if}

    <ul class="rows" aria-label="Missing tracks and their matches">
      {#each relink.rows as row (row.track_id)}
        {@const cand = relink.candidate(row)}
        {@const sel = relink.sel[row.track_id]}
        <li class="row" class:off={!cand}>
          <div class="top">
            <label class="tick">
              <input type="checkbox" disabled={!cand} checked={!!sel?.ticked && !!cand} onchange={() => relink.toggle(row.track_id)} />
              <span class="label">{row.label}</span>
            </label>
            {#if cand}<span class="chip {confClass[cand.confidence]}">{confidenceLabel(cand.confidence)}</span>{/if}
          </div>
          <div class="was muted">Now: {row.location}, but there is no file there{row.in_mixes.length ? `. Used by ${row.in_mixes.join(", ")}` : ""}</div>
          {#if cand}
            <div class="found">
              <span class="ev">{kindLabel(cand.kind)}</span>
              <span class="file">{cand.path}</span>
              <span class="num muted">{sizeText(cand.size)}</span>
            </div>
            <div class="to">{actionLine(cand, session.library?.kind)}</div>
            <div class="to muted">{locationLine(row.location, cand.new_location)}</div>
            {#if cand.notes.length}
              <ul class="notes">{#each cand.notes as n (n)}<li>{n}</li>{/each}</ul>
            {/if}
            {#if row.candidates.length > 1 && !sel?.manual}
              <label class="alt muted">
                Other files that fit:
                <select value={sel?.idx ?? 0} onchange={(e) => relink.choose(row.track_id, Number(e.currentTarget.value))}>
                  {#each row.candidates as c, i (c.path)}
                    <option value={i}>{i === 0 ? "Best: " : ""}{c.path} ({sizeText(c.size)})</option>
                  {/each}
                </select>
              </label>
            {/if}
          {:else}
            <div class="none">No file in this folder fits this track.</div>
            {#each row.notes as n (n)}<div class="note small">{n}</div>{/each}
          {/if}
          <div class="pick">
            <Button small disabled={relink.inspecting === row.track_id} onclick={() => relink.pickFile(root, row)}>
              {relink.inspecting === row.track_id ? "Reading…" : cand ? "Use another file…" : "Pick a file…"}
            </Button>
            {#if sel?.manual}
              <Button small variant="ghost" onclick={() => relink.choose(row.track_id, 0)} disabled={row.candidates.length === 0}>
                Back to the found file
              </Button>
            {/if}
          </div>
        </li>
      {/each}
    </ul>
    {#if chosen.length > 0}
      <p class="sum">{totalsLine(tot, session.library?.kind)}</p>
      {#if spaceProblem}<p class="err" role="alert">{spaceProblem}</p>{/if}
    {/if}
  {:else if relink.stage === "confirm"}
    <p>
      Relink {plural(chosen.length, "track")}? Nothing is deleted or overwritten. A copy of music.db is saved first, so
      Restore from a snapshot puts the library rows back; files copied onto {session.place.card} stay and then show up under unused
      files. A track's format and length are updated where the file it now uses differs; its bit rate and sample rate stay as recorded.
    </p>
    <p class="sum">{totalsLine(tot, session.library?.kind)}</p>
    {#if oddPaths}<p class="note">{oddPaths}</p>{/if}
    {#if spaceProblem}<p class="err" role="alert">{spaceProblem}</p>{/if}
    <ul class="rows plain" aria-label="Tracks to relink">
      {#each chosen as c (c.row.track_id)}
        <li class="row">
          <div class="label">{confirmLine(c.row.label, c.cand)}</div>
          {#if c.cand.confidence !== "sure" || c.manual}
            <div class="note small">{c.manual ? "You picked this file" : c.cand.confidence === "check" ? "Check this one" : "Worth knowing"}: {c.cand.notes.join(" ")}</div>
          {/if}
        </li>
      {/each}
    </ul>
  {/if}

  {#snippet actions()}
    {#if relink.stage === "choose"}
      <Button onclick={() => relink.close()}>Cancel</Button>
      <Button variant="primary" disabled={!s || !relink.folder.trim() || missing === 0} onclick={() => relink.search(root)}>Search</Button>
    {:else if relink.stage === "scanning"}
      <Button data-autofocus onclick={() => relink.cancelSearch()}>Stop searching</Button>
    {:else if relink.stage === "review"}
      <Button onclick={() => relink.clear()}>Search another folder</Button>
      <Button onclick={() => relink.close()}>Close</Button>
      <Button variant="primary" disabled={chosen.length === 0 || device.blocked || !!spaceProblem} onclick={() => relink.review()}>
        Review {plural(chosen.length, "track")}…
      </Button>
    {:else if relink.stage === "confirm"}
      <Button data-autofocus onclick={() => relink.backToList()}>Back</Button>
      <Button variant="primary" disabled={relink.busy || device.blocked || !!spaceProblem} onclick={() => relink.run(root)}>
        {relink.busy ? "Starting…" : `Relink ${plural(chosen.length, "track")}`}
      </Button>
    {/if}
  {/snippet}
</WideModal>

<style>
  p {
    margin: 0 0 8px;
  }
  .small {
    font-size: 12px;
  }
  .head {
    font-weight: 600;
    margin-bottom: 2px;
  }
  .folder {
    display: flex;
    gap: 6px;
    flex-wrap: wrap;
    margin: 10px 0;
  }
  .path {
    flex: 1 1 320px;
    min-width: 0;
    background: var(--deep);
    color: var(--text);
    border: 1px solid var(--line-hi);
    border-radius: 4px;
    padding: 5px 8px;
    font: inherit;
  }
  .opts {
    border: 1px solid var(--line);
    border-radius: 4px;
    padding: 6px 10px;
  }
  .opts summary {
    cursor: pointer;
  }
  select {
    background: var(--deep);
    color: var(--text);
    border: 1px solid var(--line-hi);
    border-radius: 4px;
    padding: 2px 6px;
    font: inherit;
    max-width: 100%;
  }
  .prog {
    margin: 10px 0;
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
    transition: width 0.2s;
  }
  .line {
    display: flex;
    justify-content: space-between;
    gap: 10px;
    margin-top: 4px;
    font-size: 12px;
  }
  .name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    min-width: 0;
  }
  .bulk {
    display: flex;
    gap: 6px;
    flex-wrap: wrap;
    margin: 6px 0;
  }
  .rows {
    list-style: none;
    margin: 6px 0 0;
    padding: 0;
    max-height: 46vh;
    overflow: auto;
    border: 1px solid var(--line);
    border-radius: 4px;
  }
  .row {
    padding: 8px 10px;
    border-bottom: 1px solid var(--line);
    content-visibility: auto;
    contain-intrinsic-size: auto 7em;
  }
  .row:last-child {
    border-bottom: 0;
  }
  .row:nth-child(odd) {
    background: var(--row);
  }
  .plain .row {
    contain-intrinsic-size: auto 2.4em;
  }
  .top {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }
  .tick {
    display: flex;
    align-items: baseline;
    gap: 8px;
    cursor: pointer;
    min-width: 0;
  }
  .label {
    font-weight: 600;
    overflow-wrap: anywhere;
  }
  .plain .label {
    font-weight: 400;
  }
  .chip {
    flex: none;
    font-size: 11px;
    padding: 1px 8px;
    border-radius: 9px;
  }
  .chip.sure {
    background: color-mix(in srgb, var(--ok) 12%, transparent);
    color: var(--ok);
  }
  .chip.likely {
    background: var(--accent-soft);
    color: var(--accent);
  }
  .chip.check {
    background: var(--bad-bg);
    color: var(--bad-text);
    border: 1px solid var(--bad);
  }
  .was {
    font-size: 11px;
    margin: 2px 0 4px 24px;
    overflow-wrap: anywhere;
  }
  .found,
  .to,
  .notes,
  .alt,
  .none,
  .pick,
  .note.small {
    margin-left: 24px;
  }
  .found {
    display: flex;
    gap: 8px;
    align-items: baseline;
    flex-wrap: wrap;
  }
  .ev {
    color: var(--green);
    font-size: 12px;
  }
  .file {
    font-family: ui-monospace, Consolas, monospace;
    font-size: 12px;
    overflow-wrap: anywhere;
    flex: 1 1 200px;
    min-width: 0;
  }
  .to {
    font-size: 12px;
    margin-top: 2px;
  }
  .notes {
    list-style: disc inside;
    padding: 0;
    margin-top: 2px;
    margin-bottom: 0;
    font-size: 12px;
    color: var(--muted);
  }
  .alt {
    display: flex;
    gap: 6px;
    align-items: center;
    flex-wrap: wrap;
    margin-top: 4px;
    font-size: 12px;
  }
  .alt select {
    flex: 1 1 260px;
    min-width: 0;
  }
  .none {
    color: var(--muted);
  }
  .pick {
    display: flex;
    gap: 6px;
    margin-top: 6px;
  }
  .off .label {
    font-weight: 400;
  }
  .sum {
    margin-top: 10px;
  }
  .note {
    font-size: 12px;
    color: var(--muted);
    border-left: 2px solid var(--line-hi);
    padding-left: 8px;
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
