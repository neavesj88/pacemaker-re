<script lang="ts">
  import { historyOverview, historySaveAsCase, type HistoryOverview, type PlayedCollection } from "$lib/api/history";
  import type { Track } from "$lib/api/types";
  import { mmss } from "$lib/format";
  import { jobs } from "$lib/state/jobs.svelte";
  import { session } from "$lib/state/session.svelte";
  import { toast } from "$lib/state/toast.svelte";
  import Button from "$lib/ui/Button.svelte";
  import Table, { type Column } from "$lib/ui/Table.svelte";
  import SaveCaseModal from "./SaveCaseModal.svelte";
  import { ago, dayLabel, sessionLabel, timeLabel } from "./when";

  type Tab = "sessions" | "most" | "never" | "collections";
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
  let missing = $derived((data?.most_played.length ?? 0) - mostRows.length);

  const trackCols: Column[] = [
    { key: "n", label: "#", num: true },
    { key: "title", label: "Title" },
    { key: "artist", label: "Artist" },
    { key: "bpm", label: "BPM", num: true },
    { key: "key", label: "Key" },
    { key: "time", label: "Length", num: true },
  ];
  const sessionCols: Column[] = trackCols.map((c) => (c.key === "n" ? { ...c, label: "Played at" } : c));
  const mostCols: Column[] = [...trackCols, { key: "plays", label: "Plays", num: true }, { key: "last", label: "Last played", num: true }];
  const collCols: Column[] = [
    { key: "name", label: "Name" },
    { key: "track_count", label: "Tracks", num: true },
    { key: "times_played", label: "Plays", num: true },
    { key: "last_played", label: "Last played", num: true },
  ];

  function ask(name: string, ids: number[], source: string) {
    if (!jobs.guard()) return;
    const known = ids.filter((id) => byId.has(id));
    if (known.length === 0) return;
    pending = { name, ids: known, source };
  }

  async function save(name: string) {
    const lib = session.library;
    if (!lib || !pending) return;
    saving = true;
    try {
      await historySaveAsCase(lib.root, name, pending.ids);
      toast.show(`Case "${name.trim()}" created with ${pending.ids.length} tracks`, "ok");
      pending = null;
      await session.load(lib.root, true);
    } catch (e) {
      session.error = String(e);
      pending = null;
    } finally {
      saving = false;
    }
  }

  const when = (ms: number) => new Date(ms).toLocaleString();
  const dateStamp = (ms: number) => dayLabel(ms).replace(/,/g, "");
  const tip = (c: PlayedCollection) => (c.last_played > 0 ? when(c.last_played) : "never");
</script>

<div class="bar">
  <h2>History</h2>
  <span class="muted">What the Pacemaker played, read back from its library</span>
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
      <b>The device has not recorded any plays yet.</b>
      Every play counter in this library is zero: no track, case or mix has ever been played on it, or it was reset. The
      Pacemaker counts a play on the device itself when it plays something; those counters are read back here each time you open
      the library. Play something on the device, reconnect it and open it again, and this page fills in. Tracks you have only
      played in this app are not counted. Until then everything is listed as never played.
    </div>
  {/if}

  <div class="tabs" role="tablist">
    <button role="tab" aria-selected={tab === "sessions"} class:on={tab === "sessions"} onclick={() => (tab = "sessions")}>
      Recently played
    </button>
    <button role="tab" aria-selected={tab === "most"} class:on={tab === "most"} onclick={() => (tab = "most")}>
      Most played <span class="n">{data.most_played.length}</span>
    </button>
    <button role="tab" aria-selected={tab === "never"} class:on={tab === "never"} onclick={() => (tab = "never")}>
      Never played <span class="n">{data.never_played.length}</span>
    </button>
    <button role="tab" aria-selected={tab === "collections"} class:on={tab === "collections"} onclick={() => (tab = "collections")}>
      Cases and mixes
    </button>
  </div>

  <div class="body">
    {#if tab === "sessions"}
      <div class="note">
        The device stores only each track's most recent play, not a log. Sessions are rebuilt from those times: a pause longer than
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
          <Table columns={sessionCols} rows={s.rows} rowKey={(r) => r.track.track_id}>
            {#snippet cell(r, c)}
              {#if c.key === "n"}<span title={when(r.at ?? 0)}>{timeLabel(r.at ?? 0)}</span>
              {:else if c.key === "title"}{r.track.title}
              {:else if c.key === "artist"}{r.track.artist}
              {:else if c.key === "bpm"}{r.track.bpm ? r.track.bpm.toFixed(1) : ""}
              {:else if c.key === "key"}{r.track.key}
              {:else}{mmss(r.track.play_time_secs)}{/if}
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
      <Table columns={mostCols} rows={mostRows} rowKey={(r) => r.track.track_id}>
        {#snippet cell(r, c)}
          {#if c.key === "n"}{mostRows.indexOf(r) + 1}
          {:else if c.key === "title"}{r.track.title}
          {:else if c.key === "artist"}{r.track.artist}
          {:else if c.key === "bpm"}{r.track.bpm ? r.track.bpm.toFixed(1) : ""}
          {:else if c.key === "key"}{r.track.key}
          {:else if c.key === "time"}{mmss(r.track.play_time_secs)}
          {:else if c.key === "plays"}{r.plays}
          {:else}<span title={r.last && r.last > 0 ? when(r.last) : "never"}>{ago(r.last ?? -1)}</span>{/if}
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
      <Table columns={trackCols} rows={neverRows} rowKey={(r) => r.track.track_id}>
        {#snippet cell(r, c)}
          {#if c.key === "n"}{neverRows.indexOf(r) + 1}
          {:else if c.key === "title"}{r.track.title}
          {:else if c.key === "artist"}{r.track.artist}
          {:else if c.key === "bpm"}{r.track.bpm ? r.track.bpm.toFixed(1) : ""}
          {:else if c.key === "key"}{r.track.key}
          {:else}{mmss(r.track.play_time_secs)}{/if}
        {/snippet}
      </Table>
      {#if neverRows.length === 0}<p class="empty">Every track has been played at least once.</p>{/if}
    {:else}
      {#each [{ title: "Cases", rows: data.cases, noun: "cases" }, { title: "Mixes", rows: data.mixes, noun: "mixes" }] as g (g.title)}
        <h4 class="sub">{g.title}</h4>
        <Table columns={collCols} rows={[...g.rows].sort((a, b) => b.times_played - a.times_played || b.last_played - a.last_played)} rowKey={(c) => c.id}>
          {#snippet cell(c, col)}
            {#if col.key === "name"}{c.name}
            {:else if col.key === "track_count"}{c.track_count}
            {:else if col.key === "times_played"}{c.times_played}
            {:else}<span title={tip(c)}>{ago(c.last_played)}</span>{/if}
          {/snippet}
        </Table>
        {#if g.rows.length === 0}<p class="empty">No {g.noun} on this device yet.</p>{/if}
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
    overflow: hidden;
  }
  .head {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  /* the same column widths in every session card, so the cards read as one list */
  .card :global(table) {
    table-layout: fixed;
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
  .grow {
    flex: 1;
  }
</style>
