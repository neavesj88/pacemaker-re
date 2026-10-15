<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
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
  type LibraryView = {
    root: string;
    problems: string[];
    tracks: Track[];
    mixes: Collection[];
    cases: Collection[];
  };
  type View = "tracks" | "mixes" | "cases";
  type SortKey = "title" | "artist" | "album" | "genre" | "bpm" | "key" | "play_time_secs";

  let volumes = $state<string[]>([]);
  let library = $state<LibraryView | null>(null);
  let error = $state("");
  let loading = $state(false);
  let view = $state<View>("tracks");
  let query = $state("");
  let sortKey = $state<SortKey>("artist");
  let sortAsc = $state(true);

  const columns: { key: SortKey; label: string; num?: boolean }[] = [
    { key: "title", label: "Title" },
    { key: "artist", label: "Artist" },
    { key: "album", label: "Album" },
    { key: "genre", label: "Genre" },
    { key: "bpm", label: "BPM", num: true },
    { key: "key", label: "Key" },
    { key: "play_time_secs", label: "Time", num: true },
  ];

  let tracks = $derived.by(() => {
    if (!library) return [];
    const q = query.trim().toLowerCase();
    const rows = q
      ? library.tracks.filter((t) =>
          [t.title, t.artist, t.album, t.genre, t.key].some((f) => f.toLowerCase().includes(q)),
        )
      : library.tracks.slice();
    const dir = sortAsc ? 1 : -1;
    rows.sort((a, b) => {
      const x = a[sortKey];
      const y = b[sortKey];
      if (typeof x === "number" && typeof y === "number") return (x - y) * dir;
      return String(x).localeCompare(String(y), undefined, { sensitivity: "base" }) * dir;
    });
    return rows;
  });

  const mmss = (s: number) => `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
  const volumeName = (root: string) => root.replace(/[\\/]+$/, "").split(/[\\/]/).pop() || root;

  async function refreshVolumes() {
    volumes = await invoke<string[]>("list_volumes");
    if (!library && volumes.length > 0) await load(volumes[0]);
  }

  async function load(root: string) {
    loading = true;
    error = "";
    try {
      library = await invoke<LibraryView>("open_library", { root });
      view = "tracks";
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

  function sortBy(key: SortKey) {
    if (sortKey === key) sortAsc = !sortAsc;
    else {
      sortKey = key;
      sortAsc = true;
    }
  }

  onMount(refreshVolumes);
</script>

<div class="app">
  <header>
    <div class="brand">Pacemaker <span>Editor</span></div>
    <div class="source">
      {#if volumes.length > 0}
        <select onchange={(e) => load((e.currentTarget as HTMLSelectElement).value)}>
          {#each volumes as v}
            <option value={v} selected={library?.root === v}>{volumeName(v)}</option>
          {/each}
        </select>
      {/if}
      <button onclick={refreshVolumes} title="Look for connected devices">Rescan</button>
      <button onclick={openFolder}>Open folder…</button>
    </div>
    {#if library}
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
        <div class="stat">
          {library.tracks.filter((t) => !t.analysed).length} not analysed
        </div>
      </nav>

      <main>
        {#if view === "tracks"}
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
              {#each tracks as t (t.track_id)}
                <tr class:dim={!t.analysed}>
                  <td>{t.title}</td>
                  <td>{t.artist}</td>
                  <td>{t.album}</td>
                  <td>{t.genre}</td>
                  <td class="num">{t.bpm ? t.bpm.toFixed(2) : "–"}</td>
                  <td>{t.key}</td>
                  <td class="num">{mmss(t.play_time_secs)}</td>
                </tr>
              {/each}
            </tbody>
          </table>
          {#if tracks.length === 0}
            <p class="empty">{query ? "No tracks match the search." : "No tracks on this device yet."}</p>
          {/if}
        {:else}
          {@const rows = view === "mixes" ? library.mixes : library.cases}
          <table>
            <thead>
              <tr><th>Name</th><th class="num">Tracks</th><th class="num">Length</th></tr>
            </thead>
            <tbody>
              {#each rows as c (c.id)}
                <tr><td>{c.name}</td><td class="num">{c.track_count}</td><td class="num">{mmss(c.play_time_secs)}</td></tr>
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
      <p>Dock it over USB: it appears as a drive and is picked up here automatically.</p>
      <p>Or open a folder that contains a <code>.Pacemaker</code> library.</p>
      <div class="actions">
        <button onclick={refreshVolumes}>Rescan</button>
        <button class="primary" onclick={openFolder}>Open folder…</button>
      </div>
    </div>
  {/if}
</div>

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
  button.primary {
    background: var(--accent);
    color: #111;
    border-color: var(--accent);
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
  .spacer {
    flex: 1;
  }
  .stat {
    color: var(--muted);
    font-size: 12px;
    padding: 0 10px;
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
  code {
    color: var(--accent);
  }
</style>
