<script lang="ts">
  import { playlogClear, playlogOverview, type LogHit, type LogRank, type LogSession, type PlayLogOverview } from "$lib/api/history";
  import type { Track } from "$lib/api/types";
  import { mmss } from "$lib/format";
  import { nav } from "$lib/state/nav.svelte";
  import { playback } from "$lib/state/playback.svelte";
  import { playlog } from "$lib/state/playlog.svelte";
  import { selection } from "$lib/state/selection.svelte";
  import { session } from "$lib/state/session.svelte";
  import { toast } from "$lib/state/toast.svelte";
  import Button from "$lib/ui/Button.svelte";
  import Modal from "$lib/ui/Modal.svelte";
  import Table, { type Column } from "$lib/ui/Table.svelte";
  import BpmKey from "./BpmKey.svelte";
  import PlayLogSettings from "./PlayLogSettings.svelte";
  import TrackLogModal from "./TrackLogModal.svelte";
  import { PERIODS, fmtBytes, groupByDay, hitNote, periodLabel, periodRange, retentionLabel, type Period } from "./playlogLogic";
  import { ago, dayLabel, sessionLabel, timeLabel } from "./when";

  // The Play log section of the History view: sessions by date, per-track history, and the most
  // played tracks of a period, all derived from the app-side log of counter changes. The History
  // view owns the "Save as case" dialog; this section asks it through `ask`.
  let {
    byId,
    ask,
  }: {
    byId: Map<number, Track>;
    ask: (name: string, ids: number[], source: string) => void;
  } = $props();

  // a library on this computer has no device of its own: its counters came from a Pacemaker via Sync
  const dev = $derived(session.isComputer ? "this library" : "the device");

  type Sub = "sessions" | "most";
  type Row = { key: string; track: Track; hit?: LogHit; rank?: LogRank };

  const GAPS = [
    { min: 60, label: "1 hour" },
    { min: 120, label: "2 hours" },
    { min: 240, label: "4 hours" },
    { min: 480, label: "8 hours" },
    { min: 720, label: "12 hours" },
  ];
  const PAGE_SESSIONS = 60;
  const PAGE_RANK = 100;

  let sub = $state<Sub>("sessions");
  let period = $state<Period>("30");
  let customFrom = $state("");
  let customTo = $state("");
  let sessionLimit = $state(PAGE_SESSIONS);
  let rankLimit = $state(PAGE_RANK);
  let data = $state<PlayLogOverview | null>(null);
  let loading = $state(false);
  let showSettings = $state(false);
  let confirmClear = $state(false);
  let clearing = $state(false);
  // One of the log-choice buttons (keep, continue another drive's log, start a new one) is running.
  let choosing = $state(false);
  let detailTrack = $state<Track | null>(null);

  let rangeError = $derived(periodRange(period, 0, customFrom, customTo).error);
  let words = $derived(periodLabel(period, customFrom, customTo));
  // The gap is a saved setting; changing it here changes it everywhere.
  let gap = $derived(playlog.settings.session_gap_minutes);
  let gapOptions = $derived(GAPS.some((g) => g.min === gap) ? GAPS : [...GAPS, { min: gap, label: `${gap} minutes` }].sort((a, b) => a.min - b.min));

  // Re-reads when the library is read again (a new snapshot), the period, gap or page size changes.
  let seq = 0;
  $effect(() => {
    const root = session.library?.root;
    void playlog.tick;
    const g = gap;
    const sl = sessionLimit;
    const rl = rankLimit;
    const p = period;
    const cf = customFrom;
    const ct = customTo;
    if (!root) {
      data = null;
      return;
    }
    const r = periodRange(p, Date.now(), cf, ct);
    if (r.error) return; // keep what is shown; the message under the picker says why
    const mine = ++seq;
    loading = true;
    playlogOverview(root, { from: r.from, to: r.to, gapMinutes: g, sessionLimit: sl, rankLimit: rl })
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

  const rowOf = (hit: LogHit): Row[] => {
    const track = byId.get(hit.track_id);
    return track ? [{ key: `${hit.track_id}-${hit.at}`, track, hit }] : [];
  };
  let sessions = $derived(
    (data?.sessions ?? []).map((s: LogSession) => ({ ...s, rows: s.hits.flatMap(rowOf) })).filter((s) => s.rows.length > 0),
  );
  let days = $derived(groupByDay(sessions));
  let ranked = $derived(
    (data?.ranking ?? []).flatMap((rank): Row[] => {
      const track = byId.get(rank.track_id);
      return track ? [{ key: String(rank.track_id), track, rank }] : [];
    }),
  );

  const baseCols: Column[] = [
    { key: "title", label: "Title" },
    { key: "artist", label: "Artist" },
    { key: "bpm", label: "BPM", num: true },
    { key: "key", label: "Key" },
    { key: "time", label: "Length", num: true },
  ];
  const actCol: Column = { key: "act", label: "" };
  const sessionCols: Column[] = [{ key: "at", label: "Played at" }, ...baseCols, { key: "plays", label: "Plays", num: true }, actCol];
  const rankCols: Column[] = [
    { key: "n", label: "#", num: true },
    ...baseCols,
    { key: "plays", label: "Plays", num: true },
    { key: "windows", label: "Seen in", num: true },
    { key: "last", label: "Last played", num: true },
    actCol,
  ];

  function pick(r: Row) {
    selection.trackIds = [r.track.track_id];
    playback.load(r.track.track_id);
  }
  function playNow(r: Row) {
    selection.trackIds = [r.track.track_id];
    playback.load(r.track.track_id, true);
  }
  const showInTracks = (r: Row) => nav.go("tracks", { "tracks.trackId": r.track.track_id });

  const when = (ms: number) => new Date(ms).toLocaleString();
  const dateStamp = (ms: number) => dayLabel(ms).replace(/,/g, "");
  const ids = (rows: Row[]) => [...new Set(rows.map((r) => r.track.track_id))];

  async function choose(what: () => Promise<void>, failed: string) {
    if (choosing) return;
    choosing = true;
    try {
      await what();
    } catch (e) {
      session.error = `${failed}: ${String(e)}`;
    } finally {
      choosing = false;
    }
  }
  const keepLog = (root: string) => choose(() => playlog.adopt(root), "The play log could not be kept");
  const startNew = (root: string) => choose(() => playlog.startNew(root), "A new play log could not be started");
  const continueFrom = (root: string, file: string) => choose(() => playlog.continueLog(root, file), "That play log could not be continued here");

  async function clearLog() {
    const root = session.library?.root;
    if (!root || clearing) return;
    clearing = true;
    try {
      await playlogClear(root);
      confirmClear = false;
      toast.show(`Play log deleted. ${session.isComputer ? "This library" : "The device"} and its counters were not changed.`, "ok");
      // With logging on, the counters as they are now become the new baseline straight away.
      await playlog.record(root);
    } catch (e) {
      session.error = `The play log could not be deleted: ${String(e)}`;
    } finally {
      clearing = false;
    }
  }
</script>

{#snippet acts(r: Row)}
  <span class="acts">
    <Button small onclick={(e: MouseEvent) => { e.stopPropagation(); playNow(r); }}>Play</Button>
    <Button small onclick={(e: MouseEvent) => { e.stopPropagation(); detailTrack = r.track; }}>History</Button>
    <Button small onclick={(e: MouseEvent) => { e.stopPropagation(); showInTracks(r); }}>Find</Button>
  </span>
{/snippet}

{#snippet trackCells(r: Row, key: string)}
  {#if key === "title"}{r.track.title}
  {:else if key === "artist"}{r.track.artist}
  {:else if key === "bpm"}<BpmKey track={r.track} field="bpm" />
  {:else if key === "key"}<BpmKey track={r.track} field="key" />
  {:else if key === "time"}{mmss(r.track.play_time_secs)}{/if}
{/snippet}

<div class="top">
  <span class="muted">
    A record of how the play counters of {dev} changed, kept by this app.
    {#if data?.has_log}
      Logging since {dayLabel(data.started_at)}, {data.events} change{data.events === 1 ? "" : "s"} recorded, {fmtBytes(data.file_bytes)},
      {retentionLabel(data.retention_days)}.
      {#if data.log_root}This log belongs to the {session.isComputer ? "folder" : "drive or path"} <b>{data.log_root}</b>.{/if}
    {/if}
  </span>
  <span class="grow"></span>
  {#if data && (data.has_log || data.problem)}
    <Button small onclick={() => (confirmClear = true)}>Delete log...</Button>
  {/if}
  <Button small active={showSettings} aria-expanded={showSettings} onclick={() => (showSettings = !showSettings)}>Log settings</Button>
</div>

{#if showSettings}<PlayLogSettings />{/if}

{#if !data}
  <p class="empty">{loading ? "Reading the play log..." : "No play log loaded."}</p>
{:else}
  {#if !data.enabled}
    <div class="note warn" role="status">
      <b>Play logging is off.</b>
      Nothing is recorded when you open a library, so new plays will not appear here. Turn it on in Log settings. {session.isComputer ? "The counters" : "The device"} keep counting either way.
    </div>
  {/if}

  {#if data.problem}
    <div class="note bad" role="alert">
      <b>The play log file could not be {data.problem_moves_aside ? "read" : "used"}.</b>
      {data.problem}
      {#if data.problem_moves_aside}
        It has not been changed. The next time a library is opened with logging on, this file is moved aside (kept, not deleted) and a new
        log starts; or use Delete log to remove it now.
      {:else}
        This is not taken as damage, so the file is left exactly as it is and nothing is logged until it can be read again (close whatever
        is holding it, or update the app if it was written by a newer version).
      {/if}
    </div>
  {:else if data.mismatch}
    <div class="note bad" role="alert">
      <b>This library does not match its play log.</b>
      Fewer than {playlog.settings.identity_min_present_pct}% of the tracks the log has seen played are here. That is {session.isComputer ? "a different library in the" : "a different card at the"}
      same {session.isComputer ? "folder" : "drive or path"}, or the same {session.isComputer ? "library" : "card"} after a clean-out or a new set. Nothing is being logged for it until you choose, so the log is
      not filled with false plays.
      <span class="row">
        <Button small variant="primary" disabled={choosing} onclick={() => session.library && keepLog(session.library.root)}>
          Keep this log and continue
        </Button>
        <Button small onclick={() => (confirmClear = true)}>Delete log and start a new one...</Button>
      </span>
      <span class="sub">
        Keep: the played tracks that are not in {session.place.thisCard} stay in the log (set aside, not deleted, and not held against {session.place.card}), and
        tracks the log has never seen start from the counters they have now, so they are not counted as plays. Nothing is deleted.
        You can also change how many tracks must match, or switch the check off, in Log settings.
      </span>
    </div>
  {:else if !data.has_log && data.candidates.length > 0}
    <div class="note" role="status">
      <b>{session.isComputer ? "This library" : "This card"} matches a play log kept for another {session.isComputer ? "folder" : "drive"}.</b>
      {#if session.isComputer}
        A log is stored per library folder, so a library that appears in a new folder has no log yet.
      {:else}
        A log is stored per drive or path, and the card does not carry an identity of its own, so a card that appears at a new drive letter has no log yet.
      {/if}
      Nothing is logged for it until you choose.
      {#each data.candidates as c (c.file)}
        <span class="cand">
          The log for <b class="inl">{c.root || "an unnamed drive"}</b>: since {dayLabel(c.started_at)}, last changed {dayLabel(c.last_change_at)},
          {c.present_tracks} of its {c.played_tracks} played tracks are in {session.place.thisCard}.
          <Button small variant="primary" disabled={choosing} onclick={() => session.library && continueFrom(session.library.root, c.file)}>
            Continue it here
          </Button>
        </span>
      {/each}
      <span class="sub">
        Continue moves that log's file to this drive (nothing is copied or deleted; it stops being the log of the old drive). Or start a
        separate log for this drive, which begins from the counters as they are now. "Ask before starting a new log" in Log settings turns this question off.
      </span>
      <span class="row">
        <Button small disabled={choosing} onclick={() => session.library && startNew(session.library.root)}>Start a new log for this drive</Button>
      </span>
    </div>
  {:else if !data.has_log}
    <div class="note" role="status">
      <b>No play log yet for this library.</b>
      {#if data.enabled}
        The first time a library is opened with logging on, the play counters at that moment become the starting point. From then on each
        change is recorded, and plays are worked out from them: open the library again after {session.isComputer ? "more plays have come in with Sync" : "a night on the device"} and it appears here.
        Plays before the starting point can only be seen as a count.
      {:else}
        Turn logging on in Log settings; the next time the library is opened its counters become the starting point.
      {/if}
    </div>
  {/if}

  {#if data.has_log && !data.problem}
    <div class="controls">
      <label>
        Period
        <select bind:value={period} aria-label="Period">
          {#each PERIODS as p (p.id)}<option value={p.id}>{p.label}</option>{/each}
        </select>
      </label>
      {#if period === "custom"}
        <label>From <input type="date" bind:value={customFrom} aria-label="From date" /></label>
        <label>To <input type="date" bind:value={customTo} aria-label="To date" /></label>
      {/if}
      <label>
        A pause of
        <select
          value={gap}
          aria-label="Pause that ends a session"
          onchange={(e) => playlog.save({ ...playlog.settings, session_gap_minutes: Number((e.currentTarget as HTMLSelectElement).value) }).catch((err) => (session.error = String(err)))}
        >
          {#each gapOptions as g (g.min)}<option value={g.min}>{g.label}</option>{/each}
        </select>
        ends a session
      </label>
      <span class="grow"></span>
      <span class="chip">{data.total_plays} play{data.total_plays === 1 ? "" : "s"}</span>
      <span class="chip">{data.sessions_total} session{data.sessions_total === 1 ? "" : "s"}</span>
    </div>
    {#if rangeError}<p class="err" role="alert">{rangeError}</p>{/if}

    <div class="seg" role="group" aria-label="Play log views">
      <button class:on={sub === "sessions"} aria-pressed={sub === "sessions"} onclick={() => (sub = "sessions")}>Sessions by date</button>
      <button class:on={sub === "most"} aria-pressed={sub === "most"} onclick={() => (sub = "most")}>Most played in this period</button>
    </div>

    <div class="legend muted">
      Times are {session.isComputer ? "the clock of the library they were read from" : "the device's clock"}. A <b>~</b> means the play is not exact (hover it for why).
      {#if data.removed_plays > 0}
        {data.removed_plays} play{data.removed_plays === 1 ? "" : "s"} of tracks no longer in the library {data.removed_plays === 1 ? "is" : "are"} not listed.
      {/if}
    </div>

    <div class:stale={loading} aria-busy={loading}>
      {#if sub === "sessions"}
        {#each days as d (d.key)}
          <h4 class="day">
            {dayLabel(d.start)}
            <span class="muted">{d.sessions.length} session{d.sessions.length === 1 ? "" : "s"}, {d.plays} play{d.plays === 1 ? "" : "s"}</span>
          </h4>
          {#each d.sessions as s (s.start)}
            <section class="card">
              <div class="head">
                <h4>{sessionLabel(s.start, s.end)}</h4>
                <span class="muted">{s.plays} play{s.plays === 1 ? "" : "s"}, {ids(s.rows).length} track{ids(s.rows).length === 1 ? "" : "s"}</span>
                <span class="grow"></span>
                <Button small onclick={() => ask(`Session ${dateStamp(s.start)}`, ids(s.rows), `the session on ${dayLabel(s.start)}`)}>
                  Save session as case
                </Button>
              </div>
              <Table columns={sessionCols} rows={s.rows} rowKey={(r) => r.key} onrowclick={pick}>
                {#snippet cell(r, c)}
                  {#if c.key === "at" && r.hit}
                    <span title={hitNote(r.hit) || when(r.hit.at)}>{r.hit.time_known ? "" : "~ "}{timeLabel(r.hit.at)}</span>
                  {:else if c.key === "plays" && r.hit}
                    <span title={hitNote(r.hit)}>{r.hit.plays}{r.hit.approx ? " ~" : ""}</span>
                  {:else if c.key === "act"}{@render acts(r)}
                  {:else}{@render trackCells(r, c.key)}{/if}
                {/snippet}
              </Table>
            </section>
          {/each}
        {:else}
          <p class="empty">No plays are logged for {words}. Plays appear after the library is opened again following {session.isComputer ? "a Sync that brought in new plays" : "a session on the device"}.</p>
        {/each}
        {#if data.sessions.length < data.sessions_total}
          <div class="more">
            <span class="muted">Showing the newest {data.sessions.length} of {data.sessions_total} sessions.</span>
            <Button small onclick={() => (sessionLimit += PAGE_SESSIONS)}>Show {Math.min(PAGE_SESSIONS, data.sessions_total - data.sessions.length)} more</Button>
          </div>
        {/if}
      {:else}
        <div class="head pad">
          <span class="muted">
            Most plays first in {words}. Showing {ranked.length} of {data.ranking_total}.
          </span>
          <span class="grow"></span>
          <Button
            small
            disabled={data.ranking_ids.length === 0}
            title="Every track of this period's ranking, not only the rows shown"
            onclick={() => ask(`Most played, ${words}`, data?.ranking_ids ?? [], `the most played list for ${words}`)}
          >
            Save all {data.ranking_ids.length} as case
          </Button>
        </div>
        <Table columns={rankCols} rows={ranked} rowKey={(r) => r.key} onrowclick={pick}>
          {#snippet cell(r, c)}
            {#if c.key === "n"}{ranked.indexOf(r) + 1}
            {:else if c.key === "plays"}{r.rank?.plays}
            {:else if c.key === "windows"}<span title="The number of times the library was read with a change to this track">{r.rank?.windows}</span>
            {:else if c.key === "last"}<span title={when(r.rank?.last ?? 0)}>{ago(r.rank?.last ?? -1)}</span>
            {:else if c.key === "act"}{@render acts(r)}
            {:else}{@render trackCells(r, c.key)}{/if}
          {/snippet}
        </Table>
        {#if ranked.length === 0}<p class="empty">No plays are logged for {words}.</p>{/if}
        {#if data.ranking.length < data.ranking_total}
          <div class="more">
            <Button small onclick={() => (rankLimit += PAGE_RANK)}>Show {Math.min(PAGE_RANK, data.ranking_total - data.ranking.length)} more</Button>
          </div>
        {/if}
      {/if}
    </div>
  {/if}

  <details class="how">
    <summary>How plays are worked out, and what cannot be known</summary>
    <ul>
      <li>
        {session.isComputer ? "A library on this computer stores" : "The Pacemaker stores"} only each track's play count and the time of its last play. Each time a library is opened here, any track whose
        counters changed is written to the log. A play is a rise in the count; its time is {session.isComputer ? "the library's" : "the device's"} last-played time.
      </li>
      <li>
        The log only sees {session.isComputer ? "the library" : "the device"} when it is opened in this app. If a track was played three times between two openings, it shows as three
        plays at its latest time (marked ~); a track played on several nights between two openings looks like one night. Open the library more often for finer detail.
      </li>
      <li>
        A session is plays whose times are within the pause chosen above; {session.isComputer ? "the library records" : "the device records"} no sessions, and a track played twice in one session
        is listed once with a count, not at each real position in the running order.
      </li>
      <li>
        Times come from {session.isComputer ? "the clock of the library they were read from" : "the device's clock"}; if it was wrong the log is wrong the same way. A count that goes down (a reset, or a track put back
        {session.isComputer ? "in the library" : "on the card"}) is counted from zero. A play of a track that was added after the last snapshot is counted from zero too.
      </li>
      <li>Plays before logging began are only a count (shown in a track's History). Plays in this app's own player are not counted: {session.isComputer ? "the counters come from the library" : "the device writes the counters"}.</li>
      <li>
        {#if session.isComputer}
          The log is stored on this computer, apart from the library. It is tied to the folder the library was opened at: a library that turns up in a new folder is offered the
          matching log of the old one (it is never merged silently), and a different library in the same folder is detected and not mixed in. The checks and their thresholds are in Log settings.
        {:else}
          The log is stored on this computer, not on the Pacemaker, and the device cannot show it. The card carries no identity the app can rely on, so the log is
          tied to the drive letter or path it was opened at: a card that turns up at a new letter is offered the matching log of the old one (it is never merged
          silently), and a different card at the same letter is detected and not mixed in. The checks and their thresholds are in Log settings.
        {/if}
      </li>
    </ul>
  </details>
{/if}

{#if detailTrack}
  <TrackLogModal track={detailTrack} onclose={() => (detailTrack = null)} />
{/if}

{#if confirmClear && data}
  <Modal title="Delete the play log" onclose={() => !clearing && (confirmClear = false)}>
    <p class="m">This deletes the play log this app keeps for this library:</p>
    <ul class="m">
      {#if data.has_log}
        <li>Logging since {when(data.started_at)}.</li>
        <li>{data.events} recorded change{data.events === 1 ? "" : "s"} ({fmtBytes(data.file_bytes)}).</li>
        {#if data.first_play_at > 0}
          <li>Plays derived from them, from {when(data.first_play_at)} to {when(data.last_play_at)}.</li>
        {/if}
      {:else}
        <li>An unreadable log file ({fmtBytes(data.file_bytes)}).</li>
      {/if}
    </ul>
    <p class="m">
      It cannot be undone. {session.isComputer ? "The library" : "The Pacemaker"}, its play counters, your cases and mixes are not changed.
      {data.enabled ? "Logging is on, so a new log starts now from the counters as they are, with no dates for earlier plays." : "Logging is off, so no new log starts."}
    </p>
    {#snippet actions()}
      <Button data-autofocus disabled={clearing} onclick={() => (confirmClear = false)}>Cancel</Button>
      <Button variant="primary" disabled={clearing} onclick={clearLog}>{clearing ? "Deleting" : "Delete the log"}</Button>
    {/snippet}
  </Modal>
{/if}

<style>
  .top {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
    padding: 10px 12px;
    font-size: 12px;
  }
  .grow {
    flex: 1;
  }
  .note {
    margin: 4px 12px 10px;
    padding: 10px 12px;
    border: 1px solid var(--line-hi);
    border-left: 3px solid var(--accent);
    border-radius: 6px;
    background: var(--panel);
    line-height: 1.5;
  }
  .note b {
    display: block;
    margin-bottom: 2px;
    color: var(--accent);
  }
  .note.bad {
    border-left-color: var(--bad);
  }
  .note.bad b {
    color: var(--bad);
  }
  .row {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    margin-top: 8px;
  }
  .sub {
    display: block;
    margin-top: 8px;
    color: var(--muted);
    font-size: 12px;
  }
  .cand {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 10px;
    margin-top: 8px;
  }
  .note b.inl {
    display: inline;
    margin: 0;
    color: var(--text);
  }
  .controls {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 14px;
    padding: 4px 12px 8px;
    font-size: 12px;
    color: var(--muted);
  }
  .controls select,
  .controls input {
    background: var(--deep);
    color: var(--text);
    border: 1px solid var(--line-hi);
    border-radius: 4px;
    padding: 2px 6px;
    font: inherit;
  }
  .chip {
    border: 1px solid var(--line-hi);
    border-radius: 10px;
    padding: 0 8px;
    font-size: 12px;
    color: var(--muted);
  }
  .err {
    margin: 0 12px 8px;
    color: var(--bad);
    font-size: 12px;
  }
  .seg {
    display: inline-flex;
    margin: 4px 12px 8px;
    border: 1px solid var(--line-hi);
    border-radius: 6px;
    overflow: hidden;
  }
  .seg button {
    background: var(--panel);
    color: var(--muted);
    border: none;
    padding: 6px 12px;
    font: inherit;
    cursor: pointer;
  }
  .seg button + button {
    border-left: 1px solid var(--line-hi);
  }
  .seg button.on {
    color: var(--text);
    background: var(--accent-soft);
  }
  .legend {
    margin: 0 12px 6px;
    font-size: 12px;
  }
  .day {
    margin: 14px 12px 0;
    display: flex;
    align-items: baseline;
    gap: 10px;
  }
  .day .muted {
    font-weight: normal;
    font-size: 12px;
  }
  .card {
    margin: 8px 12px;
    background: var(--panel);
    border: 1px solid var(--line);
    border-radius: 8px;
    overflow: hidden;
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
  .more {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 12px;
  }
  .stale {
    opacity: 0.6;
  }
  .acts {
    display: inline-flex;
    gap: 4px;
  }
  .how {
    margin: 16px 12px 0;
    color: var(--muted);
    font-size: 12px;
  }
  .how summary {
    cursor: pointer;
  }
  .how ul {
    margin: 8px 0 0;
    padding-left: 18px;
    line-height: 1.5;
  }
  .m {
    margin: 0 0 10px;
    color: var(--muted);
  }
  ul.m {
    padding-left: 18px;
  }
</style>
