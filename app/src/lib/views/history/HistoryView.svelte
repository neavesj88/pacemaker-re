<script lang="ts">
  import { historyOverview, historySaveAsCase, type HistoryOverview, type PlayedCollection } from "$lib/api/history";
  import type { Track } from "$lib/api/types";
  import { mmss } from "$lib/format";
  import { metadataEditor } from "$lib/state/metadata.svelte";
  import { nav } from "$lib/state/nav.svelte";
  import { playback } from "$lib/state/playback.svelte";
  import { selection } from "$lib/state/selection.svelte";
  import { jobs } from "$lib/state/jobs.svelte";
  import { session } from "$lib/state/session.svelte";
  import { toast } from "$lib/state/toast.svelte";
  import Button from "$lib/ui/Button.svelte";
  import Table, { type Column } from "$lib/ui/Table.svelte";
  import BpmKey from "./BpmKey.svelte";
  import PlayLog from "./PlayLog.svelte";
  import SaveCaseModal from "./SaveCaseModal.svelte";
  import { ago, dayLabel, sessionLabel, timeLabel } from "./when";

  type Tab = "sessions" | "most" | "never" | "collections" | "log";
  type Row = { track: Track; plays?: number; last?: number; at?: number };
  // What the open "Save as case" dialog will create.
  type Pending = { name: string; ids: number[]; source: string };

  const GAPS = [
    { min: 60, label: "1 hour" },
    { min: 120, label: "2 hours" },
    { min: 240, label: "4 hours" },
    { min: 480, label: "8 hours" },
    { min: 720, label: "12 hours" },
  ];

  let tab = $state<Tab>("sessions");
  let gap = $state(240);
  let data = $state<HistoryOverview | null>(null);
  let loading = $state(false);
  let pending = $state<Pending | null>(null);
  let saving = $state(false);
  let saveError = $state("");
  // The Never played list can be the whole library; render it in pages.
  const PAGE = 300;
  let neverShown = $state(PAGE);

  let byId = $derived(new Map((session.library?.tracks ?? []).map((t) => [t.track_id, t])));

  // Reads the counters when the library (re)loads or the session gap changes.
  let seq = 0;
  $effect(() => {
    const root = session.library?.root;
    const g = gap;
    void session.library; // an in-place reload after a save must refresh the counters too
    if (!root) {
      data = null;
      return;
    }
    const mine = ++seq;
    loading = true;
    historyOverview(root, g)
      .then((d) => {
        if (mine === seq) data = d;
      })
      .catch((e) => {
        if (mine === seq) session.error = String(e);
      })
      .finally(() => {
        if (mine === seq) loading = false;
      });
  });

  const rowsOf = (ids: number[], extra?: (id: number) => Partial<Row>): Row[] =>
    ids.flatMap((id) => {
      const track = byId.get(id);
      return track ? [{ track, ...extra?.(id) }] : [];
    });

  let sessions = $derived(
    (data?.sessions ?? []).map((s) => {
      const at = new Map(s.track_ids.map((id, i) => [id, s.played_at[i]]));
      return { ...s, rows: rowsOf(s.track_ids, (id) => ({ at: at.get(id) })) };
    }),
  );
  let mostRows = $derived.by(() => {
    const m = new Map((data?.most_played ?? []).map((e) => [e.track_id, e]));
    return rowsOf([...m.keys()], (id) => ({ plays: m.get(id)!.times_played, last: m.get(id)!.last_played }));
  });
  let neverRows = $derived(rowsOf(data?.never_played ?? []));
  let neverVisible = $derived(neverRows.slice(0, neverShown));
  let missing = $derived((data?.most_played.length ?? 0) - mostRows.length);

  let mostIndex = $derived(new Map(mostRows.map((r, i) => [r.track.track_id, i + 1])));
  let neverIndex = $derived(new Map(neverRows.map((r, i) => [r.track.track_id, i + 1])));

  const baseCols: Column[] = [
    { key: "n", label: "#", num: true },
    { key: "title", label: "Title" },
    { key: "artist", label: "Artist" },
    { key: "bpm", label: "BPM", num: true },
    { key: "key", label: "Key" },
    { key: "time", label: "Length", num: true },
  ];
  const actCol: Column = { key: "act", label: "" };
  const trackCols: Column[] = [...baseCols, actCol];
  const sessionCols: Column[] = trackCols.map((c) => (c.key === "n" ? { ...c, label: "Played at" } : c));
  const mostCols: Column[] = [...baseCols, { key: "plays", label: "Plays", num: true }, { key: "last", label: "Last played", num: true }, actCol];
  const collCols: Column[] = [
    { key: "name", label: "Name" },
    { key: "track_count", label: "Tracks", num: true },
    { key: "times_played", label: "Plays", num: true },
    { key: "last_played", label: "Last played", num: true },
  ];

  // Rows act on the track: click selects it and loads it in the player; the buttons play it,
  // open the metadata editor, or jump to it in the Tracks view.
  function pick(r: Row) {
    selection.trackIds = [r.track.track_id];
    playback.load(r.track.track_id);
  }
  function playNow(r: Row) {
    selection.trackIds = [r.track.track_id];
    playback.load(r.track.track_id, true);
  }
  const showInTracks = (r: Row) => nav.go("tracks", { "tracks.trackId": r.track.track_id });
  const openColl = (view: "cases" | "mixes", c: PlayedCollection) =>
    nav.go(view, { [view === "cases" ? "cases.caseId" : "mixes.mixId"]: c.id });

  const TABS: { id: Tab; label: string }[] = [
    { id: "sessions", label: "Recently played" },
    { id: "most", label: "Most played" },
    { id: "never", label: "Never played" },
    { id: "collections", label: "Cases and mixes" },
    { id: "log", label: "Play log" },
  ];
  function tabKey(e: KeyboardEvent) {
    const d = e.key === "ArrowRight" ? 1 : e.key === "ArrowLeft" ? -1 : 0;
    const home = e.key === "Home" ? 0 : e.key === "End" ? TABS.length - 1 : -1;
    if (!d && home < 0) return;
    e.preventDefault();
    const i = home >= 0 ? home : (TABS.findIndex((t) => t.id === tab) + d + TABS.length) % TABS.length;
    tab = TABS[i].id;
    document.getElementById(`hist-tab-${tab}`)?.focus();
  }

  function ask(name: string, ids: number[], source: string) {
    if (!jobs.guard()) return;
    const known = ids.filter((id) => byId.has(id));
    if (known.length === 0) return;
    saveError = "";
    pending = { name, ids: known, source };
  }

  async function save(name: string) {
    const lib = session.library;
    if (!lib || !pending || saving) return;
    saving = true;
    saveError = "";
    const count = pending.ids.length;
    try {
      await historySaveAsCase(lib.root, name, pending.ids);
    } catch (e) {
      // Keep the dialog open with the typed name so the user can retry or cancel.
      saveError = String(e);
      saving = false;
      return;
    }
    toast.show(`Case "${name.trim()}" created with ${count} tracks`, "ok");
    pending = null;
    saving = false;
    try {
      await session.load(lib.root, true);
    } catch (e) {
      session.error = `The case was created, but reloading the library failed: ${String(e)}`;
    }
  }

  const when = (ms: number) => new Date(ms).toLocaleString();
  const dateStamp = (ms: number) => dayLabel(ms).replace(/,/g, "");
  const tip = (c: PlayedCollection) => (c.last_played > 0 ? when(c.last_played) : "never");
</script>

{#snippet acts(r: Row)}
  <span class="acts">
    <Button small onclick={(e: MouseEvent) => { e.stopPropagation(); playNow(r); }}>Play</Button>
    <Button small onclick={(e: MouseEvent) => { e.stopPropagation(); metadataEditor.edit([r.track.track_id]); }}>Edit</Button>
    <Button small onclick={(e: MouseEvent) => { e.stopPropagation(); showInTracks(r); }}>Find</Button>
  </span>
{/snippet}

<div class="bar">
  <h2>History</h2>
  <span class="muted">{session.isComputer ? "Plays are counted by the Pacemaker itself, not by this app" : "What the Pacemaker played, read back from its library"}</span>
  {#if data?.any_plays}
    <span class="chip">{data.total_plays} plays</span>
    <span class="chip">{data.tracks_played} of {data.tracks_total} tracks played</span>
  {/if}
</div>

{#if !session.library}
  <p class="empty">Open a library to see its play history.</p>
{:else if !data}
  <p class="empty">{loading ? "Reading play counters..." : "No history loaded."}</p>
{:else}
  {#if !data.any_plays}
    <div class="none" role="status">
      {#if session.isComputer}
        <b>No plays are recorded in this library yet.</b>
        Plays are counted on the Pacemaker, not by this app. Use Get from Pacemaker in Sync to bring them here (the setting for play
        counts decides how they are merged). Until then everything is listed as never played.
      {:else}
      <b>The device has not recorded any plays yet.</b>
      Every play counter in this library is zero: no track, case or mix has ever been played on it, or it was reset. The
      Pacemaker counts a play on the device itself when it plays something; those counters are read back here each time you open
      the library. Play something on the device, reconnect it and open it again, and this page fills in. Tracks you have only
      played in this app are not counted. Until then everything is listed as never played.
      {/if}
    </div>
  {/if}

  <div class="tabs" role="tablist" aria-label="History views">
    {#each TABS as t (t.id)}
      <button
        id="hist-tab-{t.id}"
        role="tab"
        aria-selected={tab === t.id}
        aria-controls="hist-panel"
        tabindex={tab === t.id ? 0 : -1}
        class:on={tab === t.id}
        onclick={() => (tab = t.id)}
        onkeydown={tabKey}
      >
        {t.label}
        {#if t.id === "most"}<span class="n">{data.most_played.length}</span>
        {:else if t.id === "never"}<span class="n">{data.never_played.length}</span>{/if}
      </button>
    {/each}
  </div>

  <div class="body" class:stale={loading} id="hist-panel" role="tabpanel" aria-labelledby="hist-tab-{tab}" aria-busy={loading}>
    {#if loading}<div class="updating" role="status">Updating...</div>{/if}
    {#if tab === "sessions"}
      <div class="note">
        {session.isComputer ? "This library stores" : "The device stores"} only each track's most recent play, not a log. Sessions are rebuilt from those times: a pause longer than
        <select bind:value={gap} aria-label="Pause that ends a session">
          {#each GAPS as g (g.min)}<option value={g.min}>{g.label}</option>{/each}
        </select>
        starts a new one. A track played on several nights shows only in the latest.
      </div>
      {#each sessions as s (s.start)}
        <section class="card">
          <div class="head">
            <h4>{sessionLabel(s.start, s.end)}</h4>
            <span class="muted">{s.rows.length} track{s.rows.length === 1 ? "" : "s"}</span>
            <span class="grow"></span>
            <Button small onclick={() => ask(`Session ${dateStamp(s.start)}`, s.track_ids, `the session on ${dayLabel(s.start)}`)}>
              Save session as case
            </Button>
          </div>
          <Table columns={sessionCols} rows={s.rows} rowKey={(r) => r.track.track_id} onrowclick={pick}>
            {#snippet cell(r, c)}
              {#if c.key === "n"}<span title={when(r.at ?? 0)}>{timeLabel(r.at ?? 0)}</span>
              {:else if c.key === "title"}{r.track.title}
              {:else if c.key === "artist"}{r.track.artist}
              {:else if c.key === "bpm"}<BpmKey track={r.track} field="bpm" />
              {:else if c.key === "key"}<BpmKey track={r.track} field="key" />
              {:else if c.key === "time"}{mmss(r.track.play_time_secs)}
              {:else}{@render acts(r)}{/if}
            {/snippet}
          </Table>
        </section>
      {:else}
        <p class="empty">{data.any_plays ? "No track has a last played time." : "No sessions to show."}</p>
      {/each}
    {:else if tab === "most"}
      <div class="head pad">
        <span class="muted">
          Highest play count first. Showing {mostRows.length}{missing > 0 ? ` (${missing} no longer in the library)` : ""}.
        </span>
        <span class="grow"></span>
        <Button small disabled={mostRows.length === 0} onclick={() => ask("Most played", mostRows.map((r) => r.track.track_id), "the most played list")}>
          Save list as case
        </Button>
      </div>
      <Table columns={mostCols} rows={mostRows} rowKey={(r) => r.track.track_id} onrowclick={pick}>
        {#snippet cell(r, c)}
          {#if c.key === "n"}{mostIndex.get(r.track.track_id)}
          {:else if c.key === "title"}{r.track.title}
          {:else if c.key === "artist"}{r.track.artist}
          {:else if c.key === "bpm"}<BpmKey track={r.track} field="bpm" />
          {:else if c.key === "key"}<BpmKey track={r.track} field="key" />
          {:else if c.key === "time"}{mmss(r.track.play_time_secs)}
          {:else if c.key === "plays"}{r.plays}
          {:else if c.key === "last"}<span title={r.last && r.last > 0 ? when(r.last) : "never"}>{ago(r.last ?? -1)}</span>
          {:else}{@render acts(r)}{/if}
        {/snippet}
      </Table>
      {#if mostRows.length === 0}<p class="empty">No track has a play count above zero.</p>{/if}
    {:else if tab === "never"}
      <div class="head pad">
        <span class="muted">{neverRows.length} track{neverRows.length === 1 ? "" : "s"} with no recorded play.</span>
        <span class="grow"></span>
        <Button small disabled={neverRows.length === 0} onclick={() => ask("Never played", neverRows.map((r) => r.track.track_id), "the never played list")}>
          Save list as case
        </Button>
      </div>
      <Table columns={trackCols} rows={neverVisible} rowKey={(r) => r.track.track_id} onrowclick={pick}>
        {#snippet cell(r, c)}
          {#if c.key === "n"}{neverIndex.get(r.track.track_id)}
          {:else if c.key === "title"}{r.track.title}
          {:else if c.key === "artist"}{r.track.artist}
          {:else if c.key === "bpm"}<BpmKey track={r.track} field="bpm" />
          {:else if c.key === "key"}<BpmKey track={r.track} field="key" />
          {:else if c.key === "time"}{mmss(r.track.play_time_secs)}
          {:else}{@render acts(r)}{/if}
        {/snippet}
      </Table>
      {#if neverRows.length > neverVisible.length}
        <div class="head pad">
          <span class="muted">Showing {neverVisible.length} of {neverRows.length}.</span>
          <Button small onclick={() => (neverShown += PAGE)}>Show {Math.min(PAGE, neverRows.length - neverVisible.length)} more</Button>
          <Button small onclick={() => (neverShown = neverRows.length)}>Show all</Button>
        </div>
      {/if}
      {#if neverRows.length === 0}<p class="empty">Every track has been played at least once.</p>{/if}
    {:else if tab === "log"}
      <PlayLog {byId} {ask} />
    {:else}
      {#each [{ title: "Cases", rows: data.cases, noun: "cases", view: "cases" as const }, { title: "Mixes", rows: data.mixes, noun: "mixes", view: "mixes" as const }] as g (g.title)}
        <h4 class="sub">{g.title}</h4>
        <Table
          columns={collCols}
          rows={[...g.rows].sort((a, b) => b.times_played - a.times_played || b.last_played - a.last_played)}
          rowKey={(c) => c.id}
          onrowclick={(c) => openColl(g.view, c)}
        >
          {#snippet cell(c, col)}
            {#if col.key === "name"}{c.name}
            {:else if col.key === "track_count"}{c.track_count}
            {:else if col.key === "times_played"}{c.times_played}
            {:else}<span title={tip(c)}>{ago(c.last_played)}</span>{/if}
          {/snippet}
        </Table>
        {#if g.rows.length === 0}<p class="empty">No {g.noun} {session.isComputer ? "in this library" : "on this device"} yet.</p>{/if}
      {/each}
    {/if}
  </div>
{/if}

{#if pending}
  <SaveCaseModal
    initialName={pending.name}
    count={pending.ids.length}
    source={pending.source}
    busy={saving}
    error={saveError}
    onsave={save}
    oncancel={() => (pending = null)}
  />
{/if}

<style>
  .bar {
    display: flex;
    align-items: baseline;
    gap: 12px;
    padding: 10px 12px;
    border-bottom: 1px solid var(--line);
  }
  h2 {
    margin: 0;
    font-size: 16px;
  }
  .chip {
    border: 1px solid var(--line-hi);
    border-radius: 10px;
    padding: 0 8px;
    font-size: 12px;
    color: var(--muted);
  }
  .none {
    margin: 12px;
    padding: 10px 12px;
    border: 1px solid var(--line-hi);
    border-left: 3px solid var(--accent);
    border-radius: 6px;
    background: var(--panel);
    line-height: 1.5;
  }
  .none b {
    display: block;
    margin-bottom: 2px;
    color: var(--accent);
  }
  .tabs {
    display: flex;
    flex-wrap: wrap;
    gap: 2px;
    padding: 0 12px;
    border-bottom: 1px solid var(--line);
  }
  .tabs button {
    background: none;
    border: none;
    border-bottom: 2px solid transparent;
    color: var(--muted);
    padding: 8px 12px;
    font: inherit;
    cursor: pointer;
  }
  .tabs button:hover {
    color: var(--text);
  }
  .tabs button.on {
    color: var(--text);
    border-bottom-color: var(--accent);
  }
  .n {
    font-size: 11px;
    color: var(--muted);
  }
  .body {
    padding: 0 0 16px;
  }
  .note {
    margin: 10px 12px;
    color: var(--muted);
    font-size: 12px;
  }
  .note select {
    padding: 1px 4px;
    font-size: 12px;
  }
  .card {
    margin: 10px 12px;
    background: var(--panel);
    border: 1px solid var(--line);
    border-radius: 8px;
    /* scrolls, not hidden: in a narrow window the row buttons sit past the card's edge */
    overflow-x: auto;
  }
  .head {
    flex-wrap: wrap;
    display: flex;
    align-items: center;
    gap: 12px;
  }
  .card .head {
    padding: 8px 10px;
  }
  .head.pad {
    padding: 10px 12px;
  }
  h4 {
    margin: 0;
  }
  h4.sub {
    margin: 14px 12px 6px;
  }
  .stale > :global(*:not(.updating)) {
    opacity: 0.55;
  }
  .updating {
    margin: 6px 12px 0;
    font-size: 12px;
    color: var(--accent);
  }
  .acts {
    display: inline-flex;
    gap: 4px;
  }
  .body {
    overflow-x: auto;
  }
  .grow {
    flex: 1;
  }
</style>
