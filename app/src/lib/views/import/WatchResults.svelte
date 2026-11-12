<script lang="ts">
  import { untrack } from "svelte";
  import type { WatchCandidate } from "$lib/api/import";
  import { jobs } from "$lib/state/jobs.svelte";
  import { session } from "$lib/state/session.svelte";
  import { watch } from "$lib/state/watch.svelte";
  import Button from "$lib/ui/Button.svelte";
  import { agoText, chosenCandidates, fmtSize, scanLines } from "./watchLogic";

  // What the last scan of the watched folders found for the open library: the new files (tick the
  // ones to add; adding opens the same question as any add and names every file), the files
  // dismissed earlier, the files left out as already there, and the log of automatic adds.
  let err = $state("");
  let chosen = $state<Set<string>>(new Set());

  // A new scan result starts with every new file ticked, except those the user had unticked that
  // are still in the list.
  let wasListed = new Set<string>();
  $effect(() => {
    watch.version;
    const list = watch.candidates;
    untrack(() => {
      const keep = new Set<string>();
      for (const c of list) if (!wasListed.has(c.path) || chosen.has(c.path)) keep.add(c.path);
      wasListed = new Set(list.map((c) => c.path));
      chosen = keep;
    });
  });

  let now = $state(Date.now());
  $effect(() => {
    const id = setInterval(() => (now = Date.now()), 30_000);
    return () => clearInterval(id);
  });

  const scan = $derived(watch.scan);
  const picked = $derived(chosenCandidates(watch.candidates, chosen));
  const lines = $derived(scan ? scanLines(scan) : []);
  const allOn = $derived(watch.candidates.length > 0 && picked.length === watch.candidates.length);
  const titleOf = (id: number) => {
    const t = session.library?.tracks.find((x) => x.track_id === id);
    return t ? `${t.title || "(untitled)"}${t.artist ? ` · ${t.artist}` : ""}` : `track ${id}`;
  };

  function toggle(c: WatchCandidate, on: boolean) {
    const next = new Set(chosen);
    if (on) next.add(c.path);
    else next.delete(c.path);
    chosen = next;
  }
  function toggleAll(on: boolean) {
    chosen = on ? new Set(watch.candidates.map((c) => c.path)) : new Set();
  }

  async function ignoreChosen() {
    err = "";
    try {
      await watch.ignore(picked);
    } catch (e) {
      err = String(e);
    }
  }
  async function unignore(paths: string[]) {
    err = "";
    try {
      await watch.unignore(paths);
    } catch (e) {
      err = String(e);
    }
  }

  const when = (ms: number) => new Date(ms).toLocaleString();
  const modeWord = (m: string) => (m === "auto" ? "automatic" : "asked");
</script>

{#if watch.watching}
  <section class="card wide">
    <header>
      <h3>New in the watched folders</h3>
      {#if scan}<span class="muted">Scanned {agoText(scan.at, now)} ({when(scan.at)})</span>{/if}
    </header>

    {#if watch.stateProblem}<p class="err" role="alert">{watch.stateProblem}</p>{/if}
    {#if err}<p class="err" role="alert">{err}</p>{/if}

    {#if !scan}
      <p class="muted">{watch.scanning ? "Scanning…" : "No scan of this library yet. Press Scan now."}</p>
    {:else}
      {#each scan.folders.filter((f) => f.error) as f (f.path)}
        <p class="err" role="alert"><b>{f.path}</b>: {f.error}</p>
      {/each}

      <p class="summary">
        {scan.audio_files} audio file{scan.audio_files === 1 ? "" : "s"} in {scan.folders.length} folder{scan.folders.length === 1 ? "" : "s"}:
        <b>{scan.candidates_total} new</b>.
      </p>
      {#if lines.length}
        <ul class="lines muted">
          {#each lines as l (l)}<li>{l}</li>{/each}
        </ul>
      {/if}

      {#if watch.candidates.length}
        <div class="bar">
          <label class="check"><input type="checkbox" checked={allOn} onchange={(e) => toggleAll(e.currentTarget.checked)} /> All {watch.candidates.length}</label>
          <span class="spacer"></span>
          <Button variant="primary" disabled={picked.length === 0 || !jobs.canWrite} onclick={() => watch.review(picked)}>
            Review and add {picked.length}…
          </Button>
          <Button disabled={picked.length === 0} onclick={ignoreChosen}>Ignore {picked.length} selected</Button>
        </div>
        {#if !jobs.canWrite}
          <p class="muted hint">Adding is not possible right now ({jobs.busy ? "another job is running" : "the library is read-only or not open"}).</p>
        {/if}
        {#if scan.candidates_total > watch.candidates.length}
          <p class="muted hint">Showing the newest {watch.candidates.length} of {scan.candidates_total}; scan again after adding to see the rest.</p>
        {/if}
        <ul class="files">
          {#each watch.candidates as c (c.path)}
            <li>
              <label class="check">
                <input type="checkbox" checked={chosen.has(c.path)} onchange={(e) => toggle(c, e.currentTarget.checked)} />
                <span class="name">{c.name}</span>
              </label>
              <span class="meta muted" title={c.path}>{c.folder} · {fmtSize(c.size)} · changed {agoText(c.modified_ms, now)}</span>
              <span class="what" class:off={!chosen.has(c.path)}>{chosen.has(c.path) ? "selected" : "not selected"}</span>
            </li>
          {/each}
        </ul>
        <p class="muted hint">
          "Ignore" only hides a file here; the file itself is never touched. Ignored files can be brought back below.
        </p>
      {:else}
        <p class="muted">Nothing new to add.</p>
      {/if}

      {#if scan.matched.length}
        <details>
          <summary>Left out because a library track matches ({scan.matched_total})</summary>
          <p class="muted hint">
            These were not offered because the name, tags or length say they are tracks the library already has. If that is wrong, add them with
            "Add tracks…" above.
          </p>
          <ul class="rows">
            {#each scan.matched as m (m.path)}
              <li><span class="t" title={m.path}>{m.name}</span><span class="why muted">{m.how}: {titleOf(m.track_id)}</span></li>
            {/each}
          </ul>
        </details>
      {/if}
    {/if}

    <details>
      <summary>Ignored files ({watch.ignored.length})</summary>
      {#if watch.ignored.length === 0}
        <p class="muted hint">Nothing is ignored.</p>
      {:else}
        <ul class="rows">
          {#each watch.ignored as i (i.path + i.size)}
            <li>
              <span class="t" title={i.path}>{i.path} <span class="muted">· {fmtSize(i.size)} · ignored {agoText(i.at, now)}</span></span>
              <span class="what">hidden from this list</span>
              <Button small variant="ghost" onclick={() => unignore([i.path])}>Stop ignoring</Button>
            </li>
          {/each}
        </ul>
        <Button small onclick={() => unignore(watch.ignored.map((i) => i.path))}>Stop ignoring all {watch.ignored.length}</Button>
      {/if}
    </details>

    <details>
      <summary>Automatic adds ({watch.log.length})</summary>
      {#if watch.log.length === 0}
        <p class="muted hint">Nothing has been added automatically for this library.</p>
      {:else}
        <ul class="rows log">
          {#each [...watch.log].reverse() as e (e.at)}
            <li>
              <details>
                <summary>
                  {when(e.at)} · {modeWord(e.mode)} · {e.requested} file{e.requested === 1 ? "" : "s"}: <span class="muted">{e.summary || "no summary"}</span>
                </summary>
                <ul class="names">
                  {#each e.names as n, k (k)}<li>{n}</li>{/each}
                </ul>
                {#if e.names.length < e.requested}<p class="muted hint">The first {e.names.length} names are kept.</p>{/if}
              </details>
            </li>
          {/each}
        </ul>
      {/if}
    </details>
  </section>
{/if}

<style>
  .card {
    padding: 12px 14px;
    background: var(--panel);
    border: 1px solid var(--line);
    border-radius: 8px;
  }
  .wide {
    grid-column: 1 / -1;
  }
  header {
    display: flex;
    align-items: baseline;
    flex-wrap: wrap;
    gap: 4px 12px;
  }
  h3 {
    margin: 0 0 6px;
    font-size: 14px;
  }
  p {
    margin: 0 0 8px;
  }
  .summary {
    margin-bottom: 4px;
  }
  .lines {
    margin: 0 0 8px;
    padding-left: 18px;
    font-size: 12px;
  }
  .bar {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
    margin: 8px 0 4px;
  }
  .spacer {
    flex: 1;
  }
  .check {
    display: flex;
    gap: 8px;
    align-items: center;
    min-width: 0;
  }
  .files,
  .rows {
    list-style: none;
    margin: 4px 0 6px;
    padding: 0;
    max-height: 320px;
    overflow: auto;
  }
  .files li,
  .rows li {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 2px 12px;
    padding: 3px 0;
    border-bottom: 1px solid var(--line);
  }
  .name,
  .t {
    min-width: 0;
    overflow-wrap: anywhere;
  }
  .name {
    font-weight: 600;
  }
  .meta {
    font-size: 12px;
    overflow-wrap: anywhere;
    flex: 1 1 200px;
  }
  .what,
  .why {
    font-size: 12px;
    color: var(--green);
  }
  .what.off {
    color: var(--muted);
  }
  .why {
    color: var(--muted);
    text-align: right;
    overflow-wrap: anywhere;
  }
  .rows .t {
    flex: 1 1 240px;
  }
  .rows.log li {
    display: block;
  }
  .names {
    margin: 4px 0;
    padding-left: 22px;
    max-height: 200px;
    overflow: auto;
    font-size: 12px;
  }
  details {
    margin: 6px 0;
  }
  summary {
    cursor: pointer;
  }
  .hint {
    margin: 4px 0;
    font-size: 12px;
  }
  .err {
    margin: 0 0 8px;
    padding: 8px 12px;
    border-radius: 6px;
    background: var(--bad-bg);
    color: var(--bad-text);
  }
</style>
