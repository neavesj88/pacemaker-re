<script lang="ts">
  import { pickWatchFolder, type WatchSettings } from "$lib/api/import";
  import { session } from "$lib/state/session.svelte";
  import { watch } from "$lib/state/watch.svelte";
  import Button from "$lib/ui/Button.svelte";
  import { agoText, decimalIn, intervalLabel, looksAbsolute, wholeIn } from "./watchLogic";

  // The watch-folder settings. Each choice saves as soon as it changes. They are kept on this
  // computer (in the app's config folder), not on the Pacemaker, and are the same for every card.
  let err = $state("");
  let busy = $state(false);
  let typed = $state("");
  let interval = $state("");
  let minAge = $state("");
  let autoMax = $state("");
  let tolerance = $state("");

  // Show a saved value in its field only when that value changed, so saving some other setting
  // does not wipe what is half typed here.
  let shown = { interval: NaN, minAge: NaN, autoMax: NaN, tolerance: NaN };
  $effect(() => {
    const s = watch.settings;
    if (s.interval_minutes !== shown.interval) interval = String((shown.interval = s.interval_minutes));
    if (s.min_age_secs !== shown.minAge) minAge = String((shown.minAge = s.min_age_secs));
    if (s.auto_max_files !== shown.autoMax) autoMax = String((shown.autoMax = s.auto_max_files));
    if (s.length_tolerance_secs !== shown.tolerance) tolerance = String((shown.tolerance = s.length_tolerance_secs));
  });

  // a clock for "3 minutes ago" that moves on without a rescan
  let now = $state(Date.now());
  $effect(() => {
    const id = setInterval(() => (now = Date.now()), 30_000);
    return () => clearInterval(id);
  });

  async function commit(patch: Partial<WatchSettings>) {
    err = "";
    busy = true;
    try {
      await watch.save({ ...watch.settings, ...patch });
    } catch (e) {
      err = String(e);
    } finally {
      busy = false;
    }
  }

  function commitNumber(raw: string, lo: number, hi: number, what: string, key: "interval_minutes" | "min_age_secs" | "auto_max_files") {
    const n = wholeIn(raw, lo, hi);
    if (n === null) {
      err = `${what} must be a whole number from ${lo} to ${hi}.`;
      if (key === "interval_minutes") interval = String(watch.settings.interval_minutes);
      else if (key === "min_age_secs") minAge = String(watch.settings.min_age_secs);
      else autoMax = String(watch.settings.auto_max_files);
      return;
    }
    err = "";
    if (n !== watch.settings[key]) void commit({ [key]: n });
  }

  function commitTolerance() {
    const n = decimalIn(tolerance, 0, 30);
    if (n === null) {
      err = "The length difference must be a number of seconds from 0 to 30.";
      tolerance = String(watch.settings.length_tolerance_secs);
      return;
    }
    err = "";
    if (n !== watch.settings.length_tolerance_secs) void commit({ length_tolerance_secs: n });
  }

  const known = (p: string) => watch.settings.folders.some((f) => f.replace(/[\\/]+$/, "").toLowerCase() === p.replace(/[\\/]+$/, "").toLowerCase());

  async function addFolder(path: string) {
    const p = path.trim();
    if (!p) return;
    if (!looksAbsolute(p)) {
      err = `"${p}" is not a full folder path (for example C:\\Music).`;
      return;
    }
    if (known(p)) {
      err = "That folder is already watched.";
      return;
    }
    await commit({ folders: [...watch.settings.folders, p] });
    if (!err) typed = "";
  }

  async function browse() {
    err = "";
    try {
      const p = await pickWatchFolder();
      if (p) await addFolder(p);
    } catch (e) {
      err = String(e);
    }
  }

  const report = (path: string) => watch.scan?.folders.find((f) => f.path === path);
  const auto = $derived(watch.settings.mode === "auto");
</script>

<section class="card wide">
  <h3>Watch folders</h3>
  <p class="muted">
    Folders on this computer that are searched for music the library does not have yet. Nothing is added unless you say so: by default new files are
    listed here and go through the usual question. These settings are kept on this computer, not on the Pacemaker.
  </p>

  {#if watch.settingsProblem}<p class="err" role="alert">{watch.settingsProblem}</p>{/if}

  <ul class="folders">
    {#each watch.settings.folders as f (f)}
      {@const r = report(f)}
      <li>
        <span class="path">{f}</span>
        {#if r?.error}
          <span class="bad">{r.error}</span>
        {:else if r}
          <span class="muted">{r.audio_files} audio file{r.audio_files === 1 ? "" : "s"}</span>
        {/if}
        <Button small variant="ghost" disabled={busy} onclick={() => commit({ folders: watch.settings.folders.filter((x) => x !== f) })} aria-label={`Stop watching ${f}`}>Stop watching</Button>
      </li>
    {:else}
      <li class="muted">No folder is watched yet.</li>
    {/each}
  </ul>

  <div class="row">
    <Button disabled={busy} onclick={browse}>Add folder…</Button>
    <input
      type="text"
      class="typed"
      placeholder="or type a full path, e.g. C:\Music"
      aria-label="Folder path to watch"
      bind:value={typed}
      disabled={busy}
      onkeydown={(e) => e.key === "Enter" && addFolder(typed)}
    />
    <Button disabled={busy || !typed.trim()} onclick={() => addFolder(typed)}>Add</Button>
  </div>

  <fieldset class="mode" disabled={busy}>
    <legend>When new files turn up</legend>
    <label class="check">
      <input type="radio" name="watch-mode" checked={!auto} onchange={() => commit({ mode: "ask" })} />
      <span><b>Ask me first</b> (default): they are listed below, and adding them opens the question that names every file.</span>
    </label>
    <label class="check">
      <input type="radio" name="watch-mode" checked={auto} onchange={() => commit({ mode: "auto" })} />
      <span>
        <b>Add them automatically</b>: up to {watch.settings.auto_max_files} files at once go onto the Pacemaker without the question. You still see a notice, the progress and a log of what was added. A bigger find is left for you to review.
      </span>
    </label>
  </fieldset>

  <div class="opts">
    <label class="check">
      <input type="checkbox" checked={watch.settings.scan_on_open} disabled={busy} onchange={(e) => commit({ scan_on_open: e.currentTarget.checked })} />
      Scan when a library is opened
    </label>
    <label class="check">
      <input type="checkbox" checked={watch.settings.subfolders} disabled={busy} onchange={(e) => commit({ subfolders: e.currentTarget.checked })} />
      Include subfolders
    </label>
    <label class="check" class:dim={auto}>
      <input
        type="checkbox"
        checked={watch.settings.open_confirmation}
        disabled={busy || auto}
        onchange={(e) => commit({ open_confirmation: e.currentTarget.checked })}
      />
      Open the question straight away when new files are found (instead of a notice)
    </label>
  </div>

  <div class="nums">
    <label class="field">
      Scan again every (minutes)
      <input
        type="text"
        inputmode="numeric"
        bind:value={interval}
        disabled={busy}
        onchange={() => commitNumber(interval, 0, 1440, "The interval", "interval_minutes")}
        onkeydown={(e) => e.key === "Enter" && commitNumber(interval, 0, 1440, "The interval", "interval_minutes")}
      />
      <span class="muted">{intervalLabel(watch.settings.interval_minutes)}; only while this app is open; 0 = off</span>
    </label>
    <label class="field">
      Wait until a file is unchanged for (seconds)
      <input
        type="text"
        inputmode="numeric"
        bind:value={minAge}
        disabled={busy}
        onchange={() => commitNumber(minAge, 0, 3600, "The waiting time", "min_age_secs")}
        onkeydown={(e) => e.key === "Enter" && commitNumber(minAge, 0, 3600, "The waiting time", "min_age_secs")}
      />
      <span class="muted">so a file still downloading or copying is left for the next scan</span>
    </label>
    <label class="field" class:dim={!auto}>
      Most files added automatically at once
      <input
        type="text"
        inputmode="numeric"
        bind:value={autoMax}
        disabled={busy || !auto}
        onchange={() => commitNumber(autoMax, 1, 5000, "The most files at once", "auto_max_files")}
        onkeydown={(e) => e.key === "Enter" && commitNumber(autoMax, 1, 5000, "The most files at once", "auto_max_files")}
      />
      <span class="muted">only used by "Add them automatically"</span>
    </label>
    <label class="field">
      Same recording if the lengths differ by at most (seconds)
      <input
        type="text"
        inputmode="decimal"
        bind:value={tolerance}
        disabled={busy}
        onchange={commitTolerance}
        onkeydown={(e) => e.key === "Enter" && commitTolerance()}
      />
      <span class="muted">used when a file's title and artist match a library track but the name does not; 0 = exactly the same length</span>
    </label>
  </div>

  {#if err}<p class="err" role="alert">{err}</p>{/if}

  <div class="row foot">
    <Button
      variant="primary"
      disabled={!session.library || !watch.watching || watch.scanning}
      onclick={() => watch.scanNow("manual")}
    >
      {watch.scanning ? "Scanning…" : "Scan now"}
    </Button>
    <span class="muted">
      {#if !session.library}
        Open a library to scan.
      {:else if !watch.watching}
        Add a folder first.
      {:else if watch.scan}
        Last scan {agoText(watch.scan.at, now)}.{auto ? " In automatic mode, files found are added." : ""}
      {:else}
        Not scanned yet.
      {/if}
    </span>
  </div>
  {#if watch.scanError}<p class="err" role="alert">{watch.scanError}</p>{/if}
</section>

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
  h3 {
    margin: 0 0 6px;
    font-size: 14px;
  }
  p {
    margin: 0 0 8px;
  }
  .folders {
    list-style: none;
    margin: 0 0 8px;
    padding: 0;
  }
  .folders li {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 4px 12px;
    padding: 4px 0;
    border-bottom: 1px solid var(--line);
  }
  .path {
    font-weight: 600;
    overflow-wrap: anywhere;
    min-width: 0;
  }
  .bad {
    color: var(--bad-text);
    font-size: 12px;
  }
  .row {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    align-items: center;
  }
  .foot {
    margin-top: 10px;
  }
  .typed {
    flex: 1 1 220px;
    min-width: 0;
  }
  input[type="text"] {
    background: var(--panel);
    color: var(--text);
    border: 1px solid var(--line);
    border-radius: 4px;
    padding: 5px 8px;
    font: inherit;
  }
  fieldset {
    margin: 12px 0 8px;
    padding: 6px 10px 8px;
    border: 1px solid var(--line);
    border-radius: 6px;
  }
  legend {
    padding: 0 6px;
    color: var(--muted);
    font-size: 12px;
  }
  .check {
    display: flex;
    gap: 8px;
    align-items: flex-start;
    padding: 2px 0;
  }
  .check input {
    margin-top: 3px;
  }
  .opts {
    display: flex;
    flex-direction: column;
    gap: 2px;
    margin-bottom: 8px;
  }
  .nums {
    display: flex;
    flex-wrap: wrap;
    gap: 10px 24px;
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: 3px;
    max-width: 300px;
  }
  .field input {
    width: 90px;
  }
  .field .muted {
    font-size: 12px;
  }
  .dim {
    opacity: 0.55;
  }
  .err {
    margin: 8px 0 0;
    padding: 8px 12px;
    border-radius: 6px;
    background: var(--bad-bg);
    color: var(--bad-text);
  }
</style>
