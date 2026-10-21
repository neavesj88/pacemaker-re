<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { getCurrentWebview } from "@tauri-apps/api/webview";
  import { open as openDialog } from "@tauri-apps/plugin-dialog";
  import { onMount } from "svelte";

  type Track = {
    track_id: number;
    title: string;
    artist: string;
    album: string;
    genre: string;
    bpm: number;
    key: string;
    play_time_secs: number;
    rating: number;
    location: string;
    file_size: number;
    format: string;
    analysed: boolean;
    times_played: number;
  };
  type Collection = { id: number; name: string; play_time_secs: number; track_count: number };
  type NeedsAnalysis = { track_id: number; reason: string };
  type LibraryView = {
    root: string;
    problems: string[];
    tracks: Track[];
    mixes: Collection[];
    cases: Collection[];
    needs_analysis: NeedsAnalysis[];
  };
  type View = "tracks" | "mixes" | "cases";
  type SortKey = "title" | "artist" | "album" | "genre" | "bpm" | "key" | "play_time_secs";

  type JobKind = "add" | "reanalyse";
  type ProgressEvent = {
    job: JobKind;
    label: string;
    done: number;
    total: number;
    status: "added" | "skipped" | "reanalysed" | "failed";
    bpm: number | null;
    error: string | null;
  };
  type DoneEvent = {
    job: JobKind;
    added: number;
    skipped: number;
    reanalysed: number;
    failed: number;
    error: string | null;
  };
  type Job = {
    kind: JobKind;
    running: boolean;
    total: number;
    done: number;
    current: string;
    bpm: number | null;
    failures: { label: string; error: string }[];
    result: DoneEvent | null;
  };
  // What the user is asked to confirm before anything is written to the device.
  type Pending = {
    kind: JobKind;
    root: string;
    count: number;
    names: string[];
    files: string[];
    trackIds: number[];
  };

  const AUDIO_EXTS = ["mp3", "flac", "wav", "m4a", "aac", "ogg", "aiff"];

  let volumes = $state<string[]>([]);
  let library = $state<LibraryView | null>(null);
  let error = $state("");
  let loading = $state(false);
  let view = $state<View>("tracks");
  let query = $state("");
  let sortKey = $state<SortKey>("artist");
  let sortAsc = $state(true);
  let job = $state<Job | null>(null);
  let pending = $state<Pending | null>(null);
  let dragging = $state(false);
  let onlyNeeds = $state(false);
  let selectedCase = $state<Collection | null>(null);
  let caseIds = $state<number[]>([]);
  let cancelBtn = $state<HTMLButtonElement | null>(null);

  const columns: { key: SortKey; label: string; num?: boolean }[] = [
    { key: "title", label: "Title" },
    { key: "artist", label: "Artist" },
    { key: "album", label: "Album" },
    { key: "genre", label: "Genre" },
    { key: "bpm", label: "BPM", num: true },
    { key: "key", label: "Key" },
    { key: "play_time_secs", label: "Time", num: true },
  ];

  // Tracks whose .str is missing, empty or unreadable (or never analysed), with the reason.
  let needs = $derived(new Map((library?.needs_analysis ?? []).map((n) => [n.track_id, n.reason])));

  function filtered(list: Track[]): Track[] {
    const q = query.trim().toLowerCase();
    const rows = list.filter(
      (t) =>
        (!onlyNeeds || needs.has(t.track_id)) &&
        (!q || [t.title, t.artist, t.album, t.genre, t.key].some((f) => f.toLowerCase().includes(q))),
    );
    const dir = sortAsc ? 1 : -1;
    rows.sort((a, b) => {
      const x = a[sortKey];
      const y = b[sortKey];
      if (typeof x === "number" && typeof y === "number") return (x - y) * dir;
      return String(x).localeCompare(String(y), undefined, { sensitivity: "base" }) * dir;
    });
    return rows;
  }

  let tracks = $derived(library ? filtered(library.tracks) : []);
  let caseTracks = $derived.by(() => {
    if (!library) return [];
    const byId = new Map(library.tracks.map((t) => [t.track_id, t]));
    return filtered(caseIds.map((id) => byId.get(id)).filter((t): t is Track => !!t));
  });
  let jobBusy = $derived(job?.running ?? false);
  let canWrite = $derived(!!library && library.problems.length === 0 && !jobBusy);

  const mmss = (s: number) => `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
  const baseName = (p: string) => p.split(/[\\/]/).pop() || p;
  const volumeName = (root: string) => root.replace(/[\\/]+$/, "").split(/[\\/]/).pop() || root;

  async function refreshVolumes() {
    // Detected devices are listed, never opened automatically: opening reads every track's .str,
    // and a docked device may have an ageing hard drive. The user chooses when to read it.
    volumes = await invoke<string[]>("list_volumes");
  }

  async function load(root: string, keepView = false) {
    loading = true;
    error = "";
    try {
      library = await invoke<LibraryView>("open_library", { root });
      if (!keepView) {
        view = "tracks";
        selectedCase = null;
        caseIds = [];
        onlyNeeds = false;
      }
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  async function openFolder() {
    const dir = await openDialog({ directory: true, title: "Choose the Pacemaker drive (the folder that contains .Pacemaker)" });
    if (typeof dir === "string") await load(dir);
  }

  async function pickCase(c: Collection) {
    if (!library) return;
    try {
      caseIds = await invoke<number[]>("case_tracks", { root: library.root, caseId: c.id });
      selectedCase = c;
    } catch (e) {
      error = String(e);
    }
  }

  // Adding: pick files or drop them, confirm the destination, then run in the background.
  async function pickFiles() {
    const sel = await openDialog({
      multiple: true,
      title: "Choose audio files to add",
      filters: [{ name: "Audio", extensions: AUDIO_EXTS }],
    });
    if (Array.isArray(sel)) await askAdd(sel);
    else if (typeof sel === "string") await askAdd([sel]);
  }

  async function askAdd(paths: string[]) {
    if (!library) return;
    if (!canWrite) {
      error = jobBusy
        ? "Another add or analysis job is still running."
        : "This library is not device-compatible, so it stays read-only here.";
      return;
    }
    // folders are searched; anything that is not audio is dropped
    const files = await invoke<string[]>("expand_audio_paths", { paths });
    if (files.length === 0) {
      error = "None of those are audio files (mp3, flac, wav, m4a, aac, ogg, aiff).";
      return;
    }
    error = "";
    pending = { kind: "add", root: library.root, count: files.length, names: files.map(baseName), files, trackIds: [] };
  }

  function askReanalyse() {
    if (!library || !canWrite || needs.size === 0) return;
    const ids = [...needs.keys()];
    const names = library.tracks.filter((t) => needs.has(t.track_id)).map((t) => t.title || baseName(t.location));
    pending = { kind: "reanalyse", root: library.root, count: ids.length, names, files: [], trackIds: ids };
  }

  async function confirmPending() {
    const p = pending;
    pending = null;
    if (!p) return;
    job = { kind: p.kind, running: true, total: p.count, done: 0, current: "", bpm: null, failures: [], result: null };
    try {
      if (p.kind === "add") await invoke("add_tracks", { root: p.root, files: p.files });
      else await invoke("reanalyse_tracks", { root: p.root, trackIds: p.trackIds });
    } catch (e) {
      job = null;
      error = String(e);
    }
  }

  function onProgress(ev: ProgressEvent) {
    if (!job || job.kind !== ev.job) return;
    job.done = ev.done;
    job.total = ev.total;
    job.current = ev.label;
    job.bpm = ev.bpm;
    if (ev.status === "failed") job.failures.push({ label: ev.label, error: ev.error ?? "failed" });
  }

  async function onDone(ev: DoneEvent) {
    if (!job || job.kind !== ev.job) return;
    job.running = false;
    job.result = ev;
    if (ev.error) error = ev.error;
    if (library) await load(library.root, true);
  }

  function summary(r: DoneEvent): string {
    const parts: string[] = [];
    if (r.added) parts.push(`${r.added} added`);
    if (r.reanalysed) parts.push(`${r.reanalysed} analysed`);
    if (r.skipped) parts.push(`${r.skipped} already on the device`);
    if (r.failed) parts.push(`${r.failed} failed`);
    return parts.join(", ") || "nothing to do";
  }

  function sortBy(key: SortKey) {
    if (sortKey === key) sortAsc = !sortAsc;
    else {
      sortKey = key;
      sortAsc = true;
    }
  }

  onMount(() => {
    refreshVolumes();
    const unlisten: (() => void)[] = [];
    let gone = false;
    const keep = (p: Promise<() => void>) => p.then((u) => (gone ? u() : unlisten.push(u))).catch(() => {});
    keep(listen<ProgressEvent>("job-progress", (e) => onProgress(e.payload)));
    keep(listen<DoneEvent>("job-done", (e) => onDone(e.payload)));
    // OS file drops arrive as Tauri webview events (not HTML5 drag events); absent in a plain browser
    try {
      keep(
        getCurrentWebview().onDragDropEvent((e) => {
          const t = e.payload.type;
          if (t === "enter" || t === "over") dragging = true;
          else if (t === "leave") dragging = false;
          else if (t === "drop") {
            dragging = false;
            askAdd(e.payload.paths);
          }
        }),
      );
    } catch {
      /* browser preview */
    }
    return () => {
      gone = true;
      unlisten.forEach((u) => u());
    };
  });

  $effect(() => {
    if (pending) cancelBtn?.focus();
  });
</script>

{#snippet trackTable(rows: Track[])}
  <table>
    <thead>
      <tr>
        {#each columns as c}
          <th class:num={c.num} onclick={() => sortBy(c.key)}>
            {c.label}{sortKey === c.key ? (sortAsc ? " ▲" : " ▼") : ""}
          </th>
        {/each}
      </tr>
    </thead>
    <tbody>
      {#each rows as t (t.track_id)}
        <tr class:dim={needs.has(t.track_id)}>
          <td>
            {t.title}
            {#if needs.has(t.track_id)}<span class="badge" title={needs.get(t.track_id)}>needs analysis</span>{/if}
          </td>
          <td>{t.artist}</td>
          <td>{t.album}</td>
          <td>{t.genre}</td>
          <td class="num">{t.bpm && !needs.has(t.track_id) ? t.bpm.toFixed(2) : "–"}</td>
          <td>{t.key}</td>
          <td class="num">{mmss(t.play_time_secs)}</td>
        </tr>
      {/each}
    </tbody>
  </table>
{/snippet}

<div class="app">
  <header>
    <div class="brand">Pacemaker <span>Revival</span></div>
    <div class="source">
      {#if volumes.length > 0}
        <select onchange={(e) => load((e.currentTarget as HTMLSelectElement).value)}>
          <option value="" disabled selected={!library}>Choose a device…</option>
          {#each volumes as v}
            <option value={v} selected={library?.root === v}>{volumeName(v)}</option>
          {/each}
        </select>
      {/if}
      <button onclick={refreshVolumes} title="Look for connected devices">Rescan</button>
      <button onclick={openFolder}>Open folder…</button>
    </div>
    {#if library}
      <button
        class="primary"
        onclick={pickFiles}
        disabled={!canWrite}
        title={canWrite
          ? "Analyse audio files and copy them to the device"
          : "Unavailable while a job runs or the library is not device-compatible"}
      >Add tracks…</button>
      <input class="search" placeholder="Search" bind:value={query} />
    {/if}
  </header>

  {#if error}
    <div class="banner bad">{error}</div>
  {/if}

  {#if library}
    {#if library.problems.length === 0}
      <div class="banner ok">
        {library.root} · music.db is device-compatible
      </div>
    {:else}
      <div class="banner bad">
        This library would not load on the device:
        <ul>
          {#each library.problems as p}<li>{p}</li>{/each}
        </ul>
      </div>
    {/if}

    {#if job}
      <div class="strip">
        {#if job.running}
          <div class="line">
            <b>{job.kind === "add" ? "Adding" : "Analysing"} {job.done} / {job.total}</b>
            <span class="cur">{job.current || "starting…"}</span>
            {#if job.bpm}<span class="bpm">{job.bpm.toFixed(2)} BPM</span>{/if}
          </div>
          <progress max={job.total} value={job.done}></progress>
        {:else if job.result}
          <div class="line">
            <b>{job.kind === "add" ? "Add finished" : "Analysis finished"}:</b>
            <span class="cur">{summary(job.result)}</span>
            <button class="small" onclick={() => (job = null)}>Dismiss</button>
          </div>
        {/if}
        {#if job.failures.length > 0}
          <ul class="fails">
            {#each job.failures as f}<li><b>{f.label}</b>: {f.error}</li>{/each}
          </ul>
        {/if}
      </div>
    {/if}

    <div class="body">
      <nav>
        <button class:active={view === "tracks"} onclick={() => (view = "tracks")}>
          Tracks <span>{library.tracks.length}</span>
        </button>
        <button class:active={view === "mixes"} onclick={() => (view = "mixes")}>
          Mixes <span>{library.mixes.length}</span>
        </button>
        <button class:active={view === "cases"} onclick={() => (view = "cases")}>
          Cases <span>{library.cases.length}</span>
        </button>
        <div class="spacer"></div>
        {#if needs.size > 0}
          <div class="stat warn">{needs.size} need analysis</div>
          <button
            class="small"
            class:active={onlyNeeds}
            onclick={() => {
              onlyNeeds = !onlyNeeds;
              if (onlyNeeds) {
                view = "tracks";
                selectedCase = null;
              }
            }}
          >{onlyNeeds ? "Show all tracks" : "Show them"}</button>
          <button
            class="small"
            onclick={askReanalyse}
            disabled={!canWrite}
            title="Rewrite the beat grid (.str) of these tracks from their audio on the device"
          >Analyse {needs.size}…</button>
        {:else}
          <div class="stat">All tracks analysed</div>
        {/if}
      </nav>

      <main>
        {#if view === "tracks"}
          {@render trackTable(tracks)}
          {#if tracks.length === 0}
            <p class="empty">
              {query
                ? "No tracks match the search."
                : onlyNeeds
                  ? "Nothing needs analysis."
                  : "No tracks on this device yet. Use Add tracks… or drop audio files here."}
            </p>
          {/if}
        {:else if view === "cases" && selectedCase}
          <div class="crumb">
            <button class="small" onclick={() => (selectedCase = null)}>← Cases</button>
            <b>{selectedCase.name}</b>
            <span>
              {caseIds.length} track{caseIds.length === 1 ? "" : "s"}{caseIds.length > caseTracks.length && !query && !onlyNeeds
                ? ` (${caseIds.length - caseTracks.length} no longer in the library)`
                : ""}
            </span>
          </div>
          {@render trackTable(caseTracks)}
          {#if caseTracks.length === 0}
            <p class="empty">{query || onlyNeeds ? "No tracks match." : "This case is empty."}</p>
          {/if}
        {:else}
          {@const rows = view === "mixes" ? library.mixes : library.cases}
          <table>
            <thead>
              <tr><th>Name</th><th class="num">Tracks</th><th class="num">Length</th></tr>
            </thead>
            <tbody>
              {#each rows as c (c.id)}
                {#if view === "cases"}
                  <tr class="click" onclick={() => pickCase(c)}>
                    <td><button class="link" onclick={(e) => (e.stopPropagation(), pickCase(c))}>{c.name}</button></td>
                    <td class="num">{c.track_count}</td>
                    <td class="num">{mmss(c.play_time_secs)}</td>
                  </tr>
                {:else}
                  <tr><td>{c.name}</td><td class="num">{c.track_count}</td><td class="num">{mmss(c.play_time_secs)}</td></tr>
                {/if}
              {/each}
            </tbody>
          </table>
          {#if rows.length === 0}
            <p class="empty">No {view} on this device yet.</p>
          {/if}
        {/if}
      </main>
    </div>
  {:else if !loading}
    <div class="welcome">
      <h1>Connect your Pacemaker</h1>
      {#if volumes.length > 0}
        <p>Found on this computer:</p>
        <div class="actions">
          {#each volumes as v}
            <button class="primary" onclick={() => load(v)}>Open {volumeName(v)}</button>
          {/each}
        </div>
      {:else}
        <p>Dock it over USB: it appears as a drive, then press Rescan.</p>
      {/if}
      <p>Or open a folder that contains a <code>.Pacemaker</code> library.</p>
      <div class="actions">
        <button onclick={refreshVolumes}>Rescan</button>
        <button onclick={openFolder}>Open folder…</button>
      </div>
    </div>
  {/if}
</div>

{#if dragging}
  <div class="dropzone">
    {library
      ? `Drop audio files to add them to ${library.root}`
      : "Open a Pacemaker library first, then drop audio files here"}
  </div>
{/if}

{#if pending}
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
  <div class="scrim" onclick={() => (pending = null)} onkeydown={(e) => e.key === "Escape" && (pending = null)}>
    <div
      class="modal"
      role="dialog"
      aria-modal="true"
      aria-label="Confirm"
      tabindex="-1"
      onclick={(e) => e.stopPropagation()}
    >
      <h2>
        {#if pending.kind === "add"}
          Add {pending.count} track{pending.count === 1 ? "" : "s"} to {pending.root} ?
        {:else}
          Re-analyse {pending.count} track{pending.count === 1 ? "" : "s"} on {pending.root} ?
        {/if}
      </h2>
      <p class="muted">
        {#if pending.kind === "add"}
          The files are analysed, copied onto the device and added to its library. Tracks already there are skipped.
        {:else}
          Only the beat grid (.str) and BPM of these tracks are rewritten, from the audio already on the device. No audio is copied.
        {/if}
      </p>
      <ul class="names">
        {#each pending.names.slice(0, 6) as n}<li>{n}</li>{/each}
        {#if pending.names.length > 6}<li class="muted">…and {pending.names.length - 6} more</li>{/if}
      </ul>
      <div class="actions end">
        <button bind:this={cancelBtn} onclick={() => (pending = null)}>Cancel</button>
        <button class="primary" onclick={confirmPending}>
          {pending.kind === "add" ? `Add ${pending.count}` : `Analyse ${pending.count}`}
        </button>
      </div>
    </div>
  </div>
{/if}

<style>
  :global(:root) {
    --bg: #161616;
    --panel: #1e1e1e;
    --row: #222;
    --row-alt: #262626;
    --line: #333;
    --text: #d8d8d8;
    --muted: #8a8a8a;
    --accent: #e8b23a;
    --ok: #4caf72;
    --bad: #d9534f;
    color-scheme: dark;
  }
  :global(html, body) {
    margin: 0;
    height: 100%;
    background: var(--bg);
    color: var(--text);
    font: 13px/1.4 system-ui, -apple-system, "Segoe UI", sans-serif;
  }
  .app {
    display: flex;
    flex-direction: column;
    height: 100vh;
  }
  header {
    display: flex;
    align-items: center;
    gap: 16px;
    padding: 8px 14px;
    background: #101010;
    border-bottom: 1px solid var(--line);
  }
  .brand {
    font-weight: 700;
    letter-spacing: 0.02em;
  }
  .brand span {
    color: var(--accent);
    font-weight: 400;
  }
  .source {
    display: flex;
    gap: 6px;
  }
  .search {
    margin-left: auto;
    width: 240px;
  }
  button,
  select,
  input {
    background: var(--panel);
    color: var(--text);
    border: 1px solid var(--line);
    border-radius: 4px;
    padding: 5px 10px;
    font: inherit;
  }
  button {
    cursor: pointer;
  }
  button:hover {
    border-color: #555;
  }
  button:disabled {
    opacity: 0.45;
    cursor: default;
  }
  button.primary {
    background: var(--accent);
    color: #111;
    border-color: var(--accent);
  }
  button.small {
    padding: 2px 8px;
    font-size: 12px;
  }
  button.link {
    background: none;
    border: none;
    padding: 0;
    color: var(--accent);
    text-align: left;
  }
  .banner {
    padding: 6px 14px;
    font-size: 12px;
    border-bottom: 1px solid var(--line);
  }
  .banner.ok {
    color: var(--muted);
  }
  .banner.ok::before {
    content: "●  ";
    color: var(--ok);
  }
  .banner.bad {
    background: #3a1d1c;
    color: #f0c2c0;
  }
  .banner ul {
    margin: 4px 0 0;
  }
  .strip {
    padding: 6px 14px 8px;
    background: #1b1b14;
    border-bottom: 1px solid var(--line);
    font-size: 12px;
  }
  .strip .line {
    display: flex;
    gap: 10px;
    align-items: center;
  }
  .strip .cur {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--muted);
  }
  .strip .bpm {
    color: var(--accent);
    font-variant-numeric: tabular-nums;
  }
  progress {
    width: 100%;
    height: 6px;
    margin-top: 4px;
    accent-color: var(--accent);
  }
  .fails {
    margin: 6px 0 0;
    padding-left: 18px;
    color: #f0c2c0;
    max-height: 90px;
    overflow: auto;
  }
  .body {
    display: flex;
    flex: 1;
    min-height: 0;
  }
  nav {
    width: 170px;
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 10px 8px;
    background: var(--panel);
    border-right: 1px solid var(--line);
  }
  nav button {
    display: flex;
    justify-content: space-between;
    border: none;
    background: none;
    text-align: left;
  }
  nav button span {
    color: var(--muted);
  }
  nav button.active {
    background: #2c2c2c;
    color: var(--accent);
  }
  nav button.small {
    justify-content: center;
    border: 1px solid var(--line);
    margin: 2px 6px;
  }
  .spacer {
    flex: 1;
  }
  .stat {
    color: var(--muted);
    font-size: 12px;
    padding: 0 10px;
  }
  .stat.warn {
    color: var(--accent);
  }
  main {
    flex: 1;
    overflow: auto;
  }
  table {
    width: 100%;
    border-collapse: collapse;
  }
  th {
    position: sticky;
    top: 0;
    background: #1a1a1a;
    text-align: left;
    font-weight: 600;
    color: var(--muted);
    padding: 6px 10px;
    border-bottom: 1px solid var(--line);
    cursor: pointer;
    user-select: none;
    white-space: nowrap;
  }
  td {
    padding: 4px 10px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    max-width: 320px;
  }
  tbody tr:nth-child(odd) {
    background: var(--row);
  }
  tbody tr:nth-child(even) {
    background: var(--row-alt);
  }
  tbody tr:hover {
    background: #303030;
  }
  tr.dim {
    color: var(--muted);
  }
  tr.click {
    cursor: pointer;
  }
  .badge {
    margin-left: 6px;
    padding: 0 6px;
    border-radius: 8px;
    font-size: 11px;
    background: #4a3a14;
    color: var(--accent);
  }
  .crumb {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 10px;
    border-bottom: 1px solid var(--line);
  }
  .crumb span {
    color: var(--muted);
  }
  .num {
    text-align: right;
    font-variant-numeric: tabular-nums;
  }
  .empty {
    color: var(--muted);
    padding: 20px;
  }
  .welcome {
    margin: auto;
    text-align: center;
    color: var(--muted);
  }
  .welcome h1 {
    color: var(--text);
    font-weight: 600;
  }
  .actions {
    display: flex;
    gap: 8px;
    justify-content: center;
    margin-top: 16px;
  }
  .actions.end {
    justify-content: flex-end;
  }
  .dropzone {
    position: fixed;
    inset: 0;
    z-index: 20;
    display: flex;
    align-items: center;
    justify-content: center;
    background: rgba(22, 22, 22, 0.85);
    border: 3px dashed var(--accent);
    color: var(--accent);
    font-size: 20px;
    pointer-events: none;
  }
  .scrim {
    position: fixed;
    inset: 0;
    z-index: 30;
    display: flex;
    align-items: center;
    justify-content: center;
    background: rgba(0, 0, 0, 0.6);
  }
  .modal {
    width: 460px;
    max-width: 90vw;
    padding: 18px 20px;
    background: var(--panel);
    border: 1px solid #444;
    border-radius: 8px;
  }
  .modal h2 {
    margin: 0 0 8px;
    font-size: 16px;
  }
  .muted {
    color: var(--muted);
  }
  .names {
    margin: 8px 0 0;
    padding-left: 18px;
    max-height: 140px;
    overflow: hidden;
  }
  code {
    color: var(--accent);
  }
</style>
