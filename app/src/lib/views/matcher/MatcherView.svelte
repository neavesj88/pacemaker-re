<script lang="ts">
  import { untrack } from "svelte";
  import { jobs } from "$lib/state/jobs.svelte";
  import { cases } from "$lib/state/cases.svelte";
  import { session } from "$lib/state/session.svelte";
  import Button from "$lib/ui/Button.svelte";
  import Modal from "$lib/ui/Modal.svelte";
  import { type Line, matcher } from "./matcher.svelte";
  import type { VersionMode } from "./matchLogic";
  import PickTrackModal from "./PickTrackModal.svelte";
  import SaveCaseModal, { type Entry } from "./SaveCaseModal.svelte";

  type Filter = "all" | "owned" | "review" | "missing";

  const lib = $derived(session.library);
  let filter = $state<Filter>("all");
  let picking = $state<Line | null>(null);
  let saving = $state(false);
  let copyFallback = $state(false);
  let showSkipped = $state(false);

  // The library changed since the list was matched (tracks added, another device opened): match
  // again, keeping the decisions that still apply.
  $effect(() => {
    if (matcher.stale && !matcher.running) untrack(() => void matcher.run());
  });

  const shown = $derived(matcher.lines.filter((l) => filter === "all" || l.resolved.status === filter));
  const edited = $derived(matcher.parsed !== null && matcher.parsedText !== matcher.text);
  const canMatch = $derived(!!lib && matcher.text.trim().length > 0 && !matcher.running && !matcher.busy);

  const STATUS: Record<Exclude<Filter, "all">, string> = { owned: "Owned", review: "Check", missing: "Missing" };
  const VERSION_HELP: Record<VersionMode, string> = {
    strict: "A different or missing mix name (Extended Mix, Radio Edit...) costs score, so those matches are asked about.",
    lenient: "A different mix name costs a little score.",
    ignore: "Mix names are not compared.",
  };

  const requested = (l: Line) => (l.item.artist ? `${l.item.artist} - ${l.item.title}` : l.item.title);

  function how(l: Line): string {
    const score = l.row.candidates.find((c) => c.track_id === l.resolved.trackId)?.score;
    switch (l.resolved.how) {
      case "auto":
        return `matched automatically, score ${score}`;
      case "confirmed":
        return `confirmed by you, score ${score}`;
      case "picked":
        return score !== undefined ? `chosen by you, score ${score}` : "chosen by you";
      case "rejected":
        return "you marked it as not owned";
      default:
        return "";
    }
  }

  const entries = $derived<Entry[]>(
    matcher.owned.map((o) => ({
      line: o.item.line,
      requested: o.item.artist ? `${o.item.artist} - ${o.item.title}` : o.item.title,
      track: matcher.trackLabel(o.trackId),
      note: how({ item: o.item, row: matcher.lines[o.item.id]?.row, choice: undefined, resolved: o.resolved }),
    })),
  );

  const defaultCaseName = $derived(`Matched: ${matcher.fileName.replace(/\.[^.]+$/, "") || "pasted list"}`);

  async function saveCase(name: string) {
    saving = false;
    await matcher.saveOwnedAsCase(name);
  }

  async function copy() {
    if (!(await matcher.copyMissing())) copyFallback = true;
  }

  function number(e: Event) {
    return (e.currentTarget as HTMLInputElement).valueAsNumber;
  }
</script>

<div class="matcher">
  {#if !lib}
    <p class="muted pad">Open a library first: the matcher compares the list with the tracks in it.</p>
  {:else}
    <section class="card">
      <h3>Request list</h3>
      <p class="muted">
        Paste lines like <code>Artist - Title</code>, or open a text, CSV, TSV or M3U file (a Spotify, Beatport or YouTube playlist saved as text or CSV works the same way).
        Nothing leaves this computer: the list is compared with the tracks on the card here.
      </p>
      <textarea
        bind:value={matcher.text}
        rows="7"
        spellcheck="false"
        aria-label="Request list"
        placeholder={"Artist - Title\nArtist - Title (Extended Mix)\nTitle Only"}
      ></textarea>
      <div class="row">
        <Button variant="primary" disabled={!canMatch} onclick={() => matcher.match()}>{matcher.parsed ? "Match again" : "Match"}</Button>
        <Button disabled={matcher.busy || matcher.running} onclick={() => matcher.openFile()}>Open file…</Button>
        <Button variant="ghost" disabled={!matcher.text && !matcher.parsed} onclick={() => matcher.clear()}>Clear</Button>
        {#if matcher.fileName}<span class="muted">from {matcher.fileName}</span>{/if}
        {#if edited}<span class="warn" role="status">The text was edited: press Match to apply.</span>{/if}
      </div>

      <details class="settings">
        <summary>Matching settings</summary>
        <div class="grid">
          <label>
            Plain lines read as
            <select value={matcher.lineOrder} onchange={(e) => matcher.setLineOrder(e.currentTarget.value === "title-artist" ? "title-artist" : "artist-title")}>
              <option value="artist-title">Artist - Title</option>
              <option value="title-artist">Title - Artist</option>
            </select>
          </label>
          <label>
            Accept without asking from score
            <input type="number" min="1" max="100" value={matcher.options.acceptAt} onchange={(e) => matcher.setOptions({ acceptAt: number(e) })} />
          </label>
          <label>
            Ask me from score
            <input type="number" min="1" max="100" value={matcher.options.reviewAt} onchange={(e) => matcher.setOptions({ reviewAt: number(e) })} />
          </label>
          <label>
            Two tracks closer than this are a tie
            <input type="number" min="0" max="50" value={matcher.options.margin} onchange={(e) => matcher.setOptions({ margin: number(e) })} />
          </label>
          <label>
            Different mix names
            <select value={matcher.options.versionMode} onchange={(e) => matcher.setOptions({ versionMode: e.currentTarget.value as VersionMode })}>
              <option value="strict">Count against the match</option>
              <option value="lenient">Count a little</option>
              <option value="ignore">Ignore</option>
            </select>
          </label>
          <label>
            Suggestions per line
            <input type="number" min="1" max="20" value={matcher.options.maxCandidates} onchange={(e) => matcher.setOptions({ maxCandidates: number(e) })} />
          </label>
        </div>
        <p class="muted">
          Score is 0-100. At or above the first number a single clear match counts as owned; between the two numbers you are asked; below the second it is missing.
          {VERSION_HELP[matcher.options.versionMode]} A line with no artist (or a library track with none) is always asked about unless the accept score is set to 80 or lower.
          These settings are remembered on this computer.
        </p>
      </details>
    </section>

    {#if matcher.error}<p class="err" role="alert">{matcher.error}</p>{/if}

    {#if matcher.running}
      <p class="muted pad">Matching… {Math.round(matcher.progress * 100)}%</p>
    {/if}

    {#if matcher.parsed && !matcher.error && matcher.lines.length}
      <section class="card results">
        <div class="summary">
          <b>{matcher.items.length}</b> request{matcher.items.length === 1 ? "" : "s"}:
          <span class="ok">{matcher.counts.owned} owned</span>,
          <span class="chk">{matcher.counts.review} to check</span>,
          <span class="bad">{matcher.counts.missing} missing</span>
          {#if matcher.parsed.merged}<span class="muted"> · {matcher.parsed.merged} repeated line{matcher.parsed.merged === 1 ? "" : "s"} merged into the first</span>{/if}
          {#if matcher.parsed.skipped.length}
            <span class="muted"> ·
              <Button variant="link" onclick={() => (showSkipped = !showSkipped)}>{matcher.parsed.skipped.length} line{matcher.parsed.skipped.length === 1 ? "" : "s"} skipped</Button></span>
          {/if}
        </div>
        {#if showSkipped}
          <ul class="skipped">
            {#each matcher.parsed.skipped as s (s.line)}
              <li><span class="muted">line {s.line}</span> {s.raw} <span class="muted">({s.reason})</span></li>
            {/each}
          </ul>
        {/if}

        <div class="filters" role="tablist" aria-label="Show">
          {#each ["all", "owned", "review", "missing"] as f (f)}
            <Button small active={filter === f} onclick={() => (filter = f as Filter)}>
              {f === "all" ? "All" : STATUS[f as Exclude<Filter, "all">]}
              ({f === "all" ? matcher.items.length : matcher.counts[f as Exclude<Filter, "all">]})
            </Button>
          {/each}
        </div>

        <div class="scroll">
          <table>
            <thead>
              <tr><th class="num">Line</th><th>Requested</th><th>Status</th><th>In your library</th><th></th></tr>
            </thead>
            <tbody>
              {#each shown as l (l.item.id)}
                {@const best = l.row.candidates[0]}
                <tr>
                  <td class="num muted">{l.item.line}</td>
                  <td class="req">
                    {requested(l)}
                    {#if l.item.repeats}<span class="muted"> (asked {l.item.repeats + 1} times)</span>{/if}
                  </td>
                  <td><span class="pill {l.resolved.status}">{STATUS[l.resolved.status]}</span></td>
                  <td class="lib">
                    {#if l.resolved.trackId !== null}
                      {matcher.trackLabel(l.resolved.trackId)}
                      <div class="sub muted">{how(l)}{#if l.resolved.how === "auto" && best?.note}; {best.note}{/if}</div>
                    {:else if l.resolved.how === "rejected"}
                      <span class="muted">{how(l)}</span>
                    {:else if best}
                      <span class="muted">{l.resolved.status === "review" ? "Best" : "Nearest"}:</span>
                      {matcher.trackLabel(best.track_id)} <span class="muted">(score {best.score})</span>
                      <div class="sub muted">{l.row.reason}{#if best.note && !l.row.reason.includes(best.note)}; {best.note}{/if}</div>
                    {:else}
                      <span class="muted">{l.row.reason}</span>
                    {/if}
                  </td>
                  <td class="act">
                    {#if l.resolved.status === "review" && best}
                      <Button small disabled={matcher.running} onclick={() => matcher.choose(l.item.id, { kind: "track", trackId: best.track_id })}>Use best</Button>
                    {/if}
                    <Button small disabled={matcher.running} onclick={() => (picking = l)}>{l.resolved.trackId === null ? "Find…" : "Change…"}</Button>
                    {#if l.resolved.status !== "missing"}
                      <Button small variant="ghost" disabled={matcher.running} onclick={() => matcher.choose(l.item.id, { kind: "missing" })}>Not owned</Button>
                    {/if}
                    {#if l.choice}
                      <Button small variant="ghost" disabled={matcher.running} onclick={() => matcher.reset(l.item.id)}>Undo</Button>
                    {/if}
                  </td>
                </tr>
              {:else}
                <tr><td colspan="5" class="muted empty">Nothing in this filter.</td></tr>
              {/each}
            </tbody>
          </table>
        </div>

        <div class="outputs">
          <div>
            <h4>Owned ({matcher.owned.length})</h4>
            <Button
              variant="primary"
              disabled={!matcher.caseIds.length || !jobs.canWrite || cases.busy}
              title={jobs.canWrite ? "" : "Writing to the library is not possible right now"}
              onclick={() => (saving = true)}>Save as case…</Button
            >
            <span class="muted">{matcher.caseIds.length} track{matcher.caseIds.length === 1 ? "" : "s"} to add, you confirm the list first.</span>
          </div>
          <div>
            <h4>Missing ({matcher.missing.length})</h4>
            <div class="row">
              <Button disabled={!matcher.missing.length} onclick={copy}>Copy list</Button>
              <Button disabled={!matcher.missing.length} onclick={() => matcher.saveMissing("txt")}>Save .txt…</Button>
              <Button disabled={!matcher.missing.length} onclick={() => matcher.saveMissing("csv")}>Save .csv…</Button>
            </div>
            <label class="inline">
              <input type="checkbox" checked={matcher.includeUndecided} onchange={(e) => matcher.setIncludeUndecided(e.currentTarget.checked)} />
              Include the {matcher.undecided} line{matcher.undecided === 1 ? "" : "s"} still waiting for your decision
            </label>
            <p class="muted note">The copied and saved list holds exactly the lines the Missing filter shows{matcher.includeUndecided ? ", plus the lines to check" : ""}, as <code>Artist - Title</code>.</p>
          </div>
        </div>
      </section>
    {:else if matcher.parsed && !matcher.error && !matcher.running}
      <p class="muted pad">
        No requests found in this text{matcher.parsed.skipped.length ? ` (${matcher.parsed.skipped.length} line${matcher.parsed.skipped.length === 1 ? "" : "s"} skipped)` : ""}.
      </p>
    {/if}
  {/if}
</div>

{#if picking}
  <PickTrackModal
    line={picking}
    onclose={() => (picking = null)}
    onpick={(id) => {
      if (picking) matcher.choose(picking.item.id, { kind: "track", trackId: id });
      picking = null;
    }}
  />
{/if}

{#if saving && lib}
  <SaveCaseModal
    root={lib.root}
    {entries}
    trackCount={matcher.caseIds.length}
    notOwned={matcher.counts.missing}
    undecided={matcher.counts.review}
    initial={defaultCaseName}
    taken={cases.infos.map((c) => c.name)}
    onconfirm={saveCase}
    onclose={() => (saving = false)}
  />
{/if}

{#if copyFallback}
  <Modal title="Copy the missing list" onclose={() => (copyFallback = false)}>
    <p class="muted">The clipboard is not available here. Select the text below and copy it.</p>
    <textarea class="plain" readonly rows="10" value={matcher.missingText} onfocus={(e) => e.currentTarget.select()}></textarea>
    {#snippet actions()}
      <Button onclick={() => (copyFallback = false)}>Close</Button>
    {/snippet}
  </Modal>
{/if}

<style>
  .matcher {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 14px;
    overflow: auto;
    height: 100%;
    box-sizing: border-box;
  }
  .card {
    padding: 12px 14px;
    background: var(--panel);
    border: 1px solid var(--line);
    border-radius: 8px;
  }
  h3 {
    margin: 0 0 6px;
    font-size: 14px;
  }
  h4 {
    margin: 0 0 6px;
    font-size: 13px;
  }
  p {
    margin: 0 0 8px;
  }
  .pad {
    padding: 8px 2px;
  }
  textarea {
    width: 100%;
    box-sizing: border-box;
    resize: vertical;
    background: var(--deep);
    color: var(--text);
    border: 1px solid var(--line);
    border-radius: 4px;
    padding: 8px 10px;
    font: 13px/1.45 ui-monospace, Consolas, monospace;
  }
  textarea:focus {
    border-color: var(--line-hi);
    outline: none;
  }
  .row {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    align-items: center;
    margin-top: 8px;
  }
  .warn {
    color: var(--cue);
  }
  .settings {
    margin-top: 10px;
  }
  summary {
    cursor: pointer;
    color: var(--muted);
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(230px, 1fr));
    gap: 8px 14px;
    margin: 8px 0;
  }
  .grid label {
    display: flex;
    flex-direction: column;
    gap: 3px;
    font-size: 12px;
    color: var(--muted);
  }
  .grid input,
  .grid select {
    background: var(--panel);
    color: var(--text);
    border: 1px solid var(--line);
    border-radius: 4px;
    padding: 4px 8px;
    font: inherit;
    font-size: 13px;
  }
  .err {
    margin: 0;
    padding: 8px 12px;
    border-radius: 6px;
    background: var(--bad-bg);
    color: var(--bad-text);
  }
  .summary {
    margin-bottom: 8px;
  }
  .ok {
    color: var(--ok);
  }
  .chk {
    color: var(--cue);
  }
  .bad {
    color: var(--bad);
  }
  .skipped {
    margin: 0 0 8px;
    padding: 6px 10px;
    list-style: none;
    max-height: 140px;
    overflow-y: auto;
    border: 1px solid var(--line);
    border-radius: 4px;
    font-size: 12px;
  }
  .filters {
    display: flex;
    gap: 6px;
    margin-bottom: 8px;
  }
  .scroll {
    max-height: 56vh;
    overflow: auto;
    border: 1px solid var(--line);
    border-radius: 4px;
  }
  table {
    width: 100%;
    border-collapse: collapse;
  }
  th {
    position: sticky;
    top: 0;
    background: var(--deep);
    text-align: left;
    font-weight: 600;
    color: var(--muted);
    padding: 6px 10px;
    border-bottom: 1px solid var(--line);
    white-space: nowrap;
  }
  td {
    padding: 5px 10px;
    vertical-align: top;
    overflow-wrap: anywhere;
  }
  tbody tr:nth-child(odd) {
    background: var(--row);
  }
  tbody tr:nth-child(even) {
    background: var(--row-alt);
  }
  .num {
    text-align: right;
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }
  .sub {
    font-size: 12px;
  }
  .act {
    white-space: nowrap;
  }
  .act :global(.btn) {
    margin-right: 4px;
  }
  .empty {
    text-align: center;
    padding: 18px;
  }
  .pill {
    display: inline-block;
    padding: 1px 8px;
    border-radius: 10px;
    border: 1px solid currentColor;
    font-size: 12px;
    white-space: nowrap;
  }
  .pill.owned {
    color: var(--ok);
  }
  .pill.review {
    color: var(--cue);
  }
  .pill.missing {
    color: var(--bad);
  }
  .outputs {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(300px, 1fr));
    gap: 14px;
    margin-top: 12px;
    padding-top: 12px;
    border-top: 1px solid var(--line);
  }
  .inline {
    display: flex;
    gap: 6px;
    align-items: center;
    margin-top: 8px;
    font-size: 13px;
  }
  .note {
    margin-top: 6px;
    font-size: 12px;
  }
  .plain {
    margin-top: 8px;
  }
</style>
