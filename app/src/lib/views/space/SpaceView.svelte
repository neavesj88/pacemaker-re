<script lang="ts">
  import { untrack } from "svelte";
  import { deviceSpaceOpenFolder, JOB_SPACE, pickArchiveFolder, type SpaceItem } from "$lib/api/device";
  import { volumeName } from "$lib/format";
  import { session } from "$lib/state/session.svelte";
  import { toast } from "$lib/state/toast.svelte";
  import Button from "$lib/ui/Button.svelte";
  import Stars from "$lib/ui/Stars.svelte";
  import Table, { type Column } from "$lib/ui/Table.svelte";
  import { device } from "../device/device.svelte";
  import JobProgress from "../device/JobProgress.svelte";
  import RunModal from "./RunModal.svelte";
  import SettingsPanel from "./SettingsPanel.svelte";
  import { space } from "./space.svelte";
  import { afterLine, bytesText, goalLine, hiddenLine, lastPlayedText, meterParts, plural, verdict, type RunMode } from "./spaceLogic";

  // Free-space planner: pick a target, see which tracks the rules nominate and why, tick, then
  // archive them to a PC folder (a verified copy) and/or remove them from the card. Nothing is
  // written until a confirmation that names every track.
  const root = $derived(session.library?.root);
  const lib = $derived(session.library);
  const detached = $derived(device.isDetached(lib));

  $effect(() => {
    const r = root;
    if (!r) return;
    untrack(() => {
      if (device.isDetached(session.library)) return;
      void space.loadSettings().then(() => space.refresh());
    });
  });

  const plan = $derived(space.plan);
  const now = Date.now();
  const totals = $derived(space.totals);
  const v = $derived(plan ? verdict(plan, totals) : null);
  const meter = $derived(plan ? meterParts(plan, space.planKind, totals.removeBytes) : null);
  const busy = $derived(device.blocked || space.previewing);
  const dir = $derived(space.settings.archive_dir);
  const canRemoveAlone = $derived(!space.settings.require_archive);
  const lastResult = $derived(device.results[JOB_SPACE]);

  const columns: Column[] = [
    { key: "tick", label: "" },
    { key: "label", label: "Track" },
    { key: "reasons", label: "Why" },
    { key: "bytes", label: "Size", num: true },
    { key: "last_played", label: "Last played" },
    { key: "rating", label: "Rating" },
  ];
  const sortable = new Set(["label", "bytes", "last_played", "rating", "reasons"]);

  function sort(key: string) {
    if (sortable.has(key)) space.sortBy(key);
  }

  /** A case a track is in is shown as a chip when the "in no case" rule is off (with it on there are none). */
  const casesChip = (i: SpaceItem): string => (i.in_cases.length ? `In ${plural(i.in_cases.length, "case")}` : "");

  async function ask(mode: RunMode) {
    if (mode !== "remove" && !dir) {
      try {
        const d = await pickArchiveFolder();
        if (!d) return;
        space.update((x) => (x.archive_dir = d));
        await space.save();
      } catch (e) {
        toast.show(String(e), "bad", 8000);
        return;
      }
    }
    await space.openConfirm(mode);
  }

  async function openArchive() {
    try {
      await deviceSpaceOpenFolder(dir);
    } catch (e) {
      toast.show(String(e), "bad", 8000);
    }
  }
</script>

<div class="bar">
  <h2>Free space</h2>
  {#if lib}
    <span class="chip">{session.isComputer ? "This computer" : volumeName(lib.root)}</span>
    <span class="sp"></span>
    <Button small disabled={space.planning || device.blocked || detached} onclick={() => space.refresh()}>
      {space.planning ? "Scanning…" : "Scan again"}
    </Button>
  {/if}
</div>

{#if lib && detached}
  <p class="muted pad" role="status">
    {volumeName(lib.root)} was ejected. It is safe to unplug the USB cable. To plan space on a Pacemaker again, plug it in
    and open it from the drive list.
  </p>
{:else if lib}
  <div class="layout">
    <aside class="side"><SettingsPanel /></aside>
    <div class="main">
      <section class="card summary" aria-live="polite">
        {#if !plan}
          <p class="muted">{space.planning ? (session.isComputer ? "Reading the library and comparing audio files…" : "Reading the card and comparing audio files…") : space.error ? "" : "Not scanned yet."}</p>
        {:else}
          <p class="goal">{goalLine(plan, space.planKind)}</p>
          {#if meter && meter.total > 0}
            <div class="meter" role="img" aria-label={meter.label}>
              {#each meter.parts as p (p.key)}<i class={p.key} style="width:{(p.bytes / meter.total) * 100}%"></i>{/each}
            </div>
            <p class="legend muted">
              <span><b class="sw used"></b>{space.planKind === "card" ? "stays (fits)" : "used"}</span>
              {#if space.planKind === "card"}<span><b class="sw over"></b>over the limit</span>{/if}
              <span><b class="sw freed"></b>ticked for removal</span>
              <span><b class="sw free"></b>{space.planKind === "card" ? "room left" : "free"}</span>
            </p>
          {/if}
          {#if v}<p class="verdict {v.tone}">{v.text}</p>{/if}
          {#if hiddenLine(plan.hidden)}<p class="note muted">{hiddenLine(plan.hidden)}</p>{/if}
          {#if plan.missing_audio > 0}
            <p class="note muted">{plural(plan.missing_audio, "track")} {plan.missing_audio === 1 ? "has" : "have"} no audio file {session.isComputer ? "in the library" : "on the card"}; the Device view's Library health card lists them.</p>
          {/if}
          {#if plan.unchecked > 0}
            <p class="note muted">{plural(plan.unchecked, "track")} {plan.unchecked === 1 ? "has" : "have"} a stored location that is not a plain path under the Music folder, so no file was looked at.</p>
          {/if}
          {#each plan.warnings as w (w)}<p class="note warn">{w}</p>{/each}
        {/if}
        {#if space.error}<p class="note bad" role="alert">{space.error}</p>{/if}
      </section>

      <section class="card list">
        <div class="tools">
          <input type="search" placeholder="Filter by name, reason or case" aria-label="Filter the list" bind:value={space.filter} />
          <Button small disabled={!plan} onclick={() => space.tickSuggested()}>Tick suggested</Button>
          <Button small disabled={!plan} onclick={() => space.tickShown()}>Tick shown</Button>
          <Button small disabled={totals.count === 0} onclick={() => space.tickNone()}>Untick all</Button>
          <span class="sp"></span>
          {#if plan}
            <span class="muted count">
              {plural(plan.items.length, "candidate")} ({bytesText(plan.candidate_bytes)}){space.filter.trim() ? `, ${space.rows.length} shown` : ""}
            </span>
          {/if}
        </div>
        <div class="scroll">
          {#if !plan}
            <p class="empty">{space.planning ? "Scanning…" : "No plan yet."}</p>
          {:else if plan.items.length === 0}
            <p class="empty">
              {plan.rules_on
                ? "No track matches the rules. Loosen a rule, or lower the protections, to see candidates."
                : "No rule is on. Turn on at least one reason to see candidates."}
            </p>
          {:else}
            <Table
              {columns}
              rows={space.rows}
              rowKey={(i) => i.track_id}
              sortKey={space.sortKey ?? undefined}
              sortAsc={space.sortAsc}
              onsort={sort}
              rowClass={(i) => (i.blocked ? "dim" : "")}
            >
              {#snippet cell(i, c)}
                {#if c.key === "tick"}
                  <input
                    type="checkbox"
                    aria-label="Tick {i.label}"
                    checked={space.ticked.has(i.track_id)}
                    onchange={() => space.toggle(i.track_id)}
                  />
                {:else if c.key === "label"}
                  <span class="name" title={i.label}>{i.label}</span>
                {:else if c.key === "reasons"}
                  <span class="chips">
                    {#each i.reasons as r (r.text)}
                      <span class="chip2 {r.kind}" title={r.text}>{r.text}</span>
                    {/each}
                    {#if casesChip(i)}<span class="chip2 cased" title={i.in_cases.join(", ")}>{casesChip(i)}</span>{/if}
                    {#if i.blocked}<span class="chip2 blocked" title={i.blocked}>{i.blocked.split(":")[0]}: archive only</span>{/if}
                  </span>
                {:else if c.key === "bytes"}
                  {bytesText(i.bytes)}
                {:else if c.key === "last_played"}
                  {lastPlayedText(i.last_played_ms, now)}{i.times_played ? ` (${i.times_played}×)` : ""}
                {:else if c.key === "rating"}
                  <Stars rating={i.rating} />
                {/if}
              {/snippet}
            </Table>
          {/if}
        </div>
      </section>

      {#if session.isComputer}
        <section class="card actions">
          <p class="muted">
            This library on this computer is your master copy, so nothing is archived or removed from it here. Use this plan to see what
            fits on a card; to free space on a Pacemaker, open that Pacemaker and use this view there.
          </p>
        </section>
      {:else}
      <section class="card actions">
        <div class="sum">
          {#if plan}
            <b>{plural(totals.count, "track")} ticked</b>
            <span class="muted">
              {bytesText(totals.bytes)}{totals.blockedCount ? `; ${plural(totals.blockedCount, "track")} in a mix can be archived but not removed` : ""}
            </span>
            <span class="muted">{afterLine(plan, space.planKind, totals.removeBytes)}</span>
          {/if}
        </div>
        <div class="buttons">
          <Button
            disabled={busy || totals.count === 0}
            title={device.blocked ? "A job is still running" : undefined}
            onclick={() => ask("archive")}
          >
            Archive…
          </Button>
          {#if canRemoveAlone}
            <Button
              disabled={busy || totals.removeCount === 0}
              title={totals.removeCount === 0 ? "Nothing ticked can be removed" : undefined}
              onclick={() => ask("remove")}
            >
              Remove…
            </Button>
          {/if}
          <Button
            variant="primary"
            disabled={busy || totals.removeCount === 0}
            title={totals.count > 0 && totals.removeCount === 0 ? "Everything ticked is in a mix: it can be archived but not removed" : undefined}
            onclick={() => ask("both")}
          >
            Archive and remove…
          </Button>
        </div>
        <JobProgress kind={JOB_SPACE} />
        {#if lastResult}
          <p class="result">
            {lastResult}
            {#if dir}<Button small variant="link" onclick={openArchive}>Open the archive folder</Button>{/if}
          </p>
        {/if}
      </section>
      {/if}
    </div>
  </div>
  <RunModal />
{:else}
  <p class="muted pad">Open a Pacemaker drive to plan its free space.</p>
{/if}

<style>
  .bar {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 7px 12px;
    border-bottom: 1px solid var(--line);
  }
  h2 {
    margin: 0 8px 0 0;
    font-size: 15px;
  }
  .chip {
    font-size: 11px;
    padding: 1px 7px;
    border-radius: 9px;
    background: color-mix(in srgb, var(--green) 10%, transparent);
    color: var(--green);
  }
  .sp {
    flex: 1;
  }
  .pad {
    padding: 20px;
  }
  .layout {
    display: grid;
    grid-template-columns: minmax(250px, 290px) minmax(0, 1fr);
    gap: 14px;
    padding: 12px;
    align-items: start;
  }
  .side {
    background: var(--panel);
    border: 1px solid var(--line);
    border-radius: 8px;
    padding: 12px;
  }
  .main {
    display: flex;
    flex-direction: column;
    gap: 12px;
    min-width: 0;
  }
  .card {
    background: var(--panel);
    border: 1px solid var(--line);
    border-radius: 8px;
    padding: 12px;
    min-width: 0;
  }
  .summary p {
    margin: 0 0 6px;
  }
  .goal {
    font-size: 14px;
  }
  .meter {
    display: flex;
    height: 14px;
    border-radius: 7px;
    overflow: hidden;
    background: var(--row-alt);
    margin: 8px 0 6px;
  }
  .meter i {
    display: block;
    height: 100%;
  }
  .used {
    background: var(--muted);
  }
  .freed {
    background: var(--accent);
  }
  .free {
    background: var(--green);
  }
  .over {
    background: var(--bad);
  }
  .legend {
    display: flex;
    flex-wrap: wrap;
    gap: 4px 14px;
    font-size: 12px;
  }
  .sw {
    display: inline-block;
    width: 10px;
    height: 10px;
    border-radius: 2px;
    margin-right: 5px;
    vertical-align: middle;
  }
  .verdict {
    font-weight: 600;
  }
  .verdict.ok {
    color: var(--green);
  }
  .verdict.warn {
    color: var(--accent);
  }
  .verdict.bad,
  .bad {
    color: var(--bad-text);
  }
  .note {
    font-size: 12px;
  }
  .note.warn {
    color: var(--accent);
  }
  .tools {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
    margin-bottom: 8px;
  }
  .tools input[type="search"] {
    min-width: 200px;
    flex: 0 1 280px;
  }
  .count {
    font-size: 12px;
  }
  .scroll {
    max-height: 52vh;
    overflow: auto;
    border: 1px solid var(--line);
    border-radius: 4px;
  }
  .scroll :global(td) {
    max-width: 380px;
    vertical-align: top;
    padding: 4px 6px;
  }
  .scroll :global(th) {
    padding: 6px;
  }
  .name {
    display: inline-block;
    max-width: 230px;
    overflow: hidden;
    text-overflow: ellipsis;
    vertical-align: bottom;
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 3px 4px;
    max-width: 250px;
  }
  .chip2 {
    white-space: nowrap;
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    font-size: 11px;
    padding: 0 6px;
    border-radius: 8px;
    border: 1px solid var(--line-hi);
    color: var(--text);
  }
  .chip2.no_case,
  .chip2.no_mix,
  .chip2.cased {
    color: var(--muted);
    border-color: var(--line);
  }
  .chip2.duplicate {
    border-color: var(--accent);
    color: var(--accent);
  }
  .chip2.blocked {
    border-color: var(--bad);
    color: var(--bad-text);
  }
  input[type="checkbox"] {
    padding: 0;
    accent-color: var(--accent);
  }
  .actions {
    position: sticky;
    bottom: 0;
  }
  .sum {
    display: flex;
    flex-wrap: wrap;
    gap: 4px 14px;
    align-items: baseline;
    margin-bottom: 8px;
  }
  .buttons {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }
  .result {
    margin: 10px 0 0;
    color: var(--muted);
    overflow-wrap: anywhere;
  }
  @media (max-width: 900px) {
    .layout {
      grid-template-columns: minmax(0, 1fr);
    }
  }
</style>
