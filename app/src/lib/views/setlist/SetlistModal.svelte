<script lang="ts">
  import { onMount, untrack } from "svelte";
  import { loadSheet, pickSetlistPath, saveSetlistHtml, saveSheet } from "$lib/api/setlist";
  import { caseTracks } from "$lib/api/cases";
  import { type MixDoc, mixOpen } from "$lib/api/mixes";
  import { cases } from "$lib/state/cases.svelte";
  import { energySource } from "$lib/state/energySource.svelte";
  import { mixes } from "$lib/state/mixes.svelte";
  import { session } from "$lib/state/session.svelte";
  import { toast } from "$lib/state/toast.svelte";
  import { keyName, parseKey } from "$lib/views/tracks/trackLogic";
  import Button from "$lib/ui/Button.svelte";
  import SetlistOptions from "./SetlistOptions.svelte";
  import { setlist } from "./setlist.svelte";
  import {
    EMPTY_SHEET,
    MAX_ENERGY,
    type Built,
    type KeyParser,
    type Sheet,
    buildDocument,
    entryOf,
    fileNameFor,
    rowsFromCase,
    rowsFromMix,
    visibleColumns,
    withEntry,
  } from "./setlistLogic";

  // The printable setlist of one case or mix: header details, a note and an energy rating per
  // track, the options, and the page itself in a preview. Printing, the preview and the saved
  // file all use the one page `buildDocument` makes. Nothing here writes to the Pacemaker: the
  // typed details are kept in the app's config folder, the page goes where the user chooses.
  let { kind, id, onclose }: { kind: "case" | "mix"; id: number; onclose: () => void } = $props();

  const root = untrack(() => session.library?.root ?? "");
  const key = untrack(() => `${kind}:${id}`);

  // ---- source ----------------------------------------------------------------------------
  let loading = $state(true);
  let failed = $state("");
  let warning = $state("");
  let name = $state("");
  let caseIds = $state<number[]>([]);
  let mixDoc = $state.raw<MixDoc | null>(null);
  let unsavedMix = $state(false);
  let sheet = $state<Sheet>({ ...EMPTY_SHEET, entries: [] });
  let ready = false;

  const byId = $derived(new Map((session.library?.tracks ?? []).map((t) => [t.track_id, t])));
  const noBpm = $derived(new Set(session.needs.keys()));
  const opts = $derived(setlist.options);
  // the energy of each track in the library, under the Energy source setting; what you type below
  // is for this setlist only and wins over it
  const libraryEnergy = (id: number) => energySource.level(id);

  const built: Built = $derived.by(() => {
    if (kind === "mix") return mixDoc ? rowsFromMix(mixDoc, byId, opts.includeMuted, noBpm, libraryEnergy) : { rows: [], totalSec: 0, notes: [] };
    return rowsFromCase(caseIds, byId, noBpm, libraryEnergy);
  });

  const parse: KeyParser = (k) => {
    const c = parseKey(k);
    return c ? { code: c.code, name: keyName(c) } : null;
  };

  const page = (screenHint: boolean) =>
    buildDocument({ name, kind, rows: built.rows, totalSec: built.totalSec, sheet, options: opts, parseKey: parse, screenHint });
  // the preview and the print: the page without the screen-only hint line
  const html = $derived(page(false));
  const shown = $derived(new Set(visibleColumns({ rows: built.rows, sheet, options: opts, parseKey: parse })));
  const repeats = $derived(built.rows.length - new Set(built.rows.map((r) => r.trackId)).size);

  onMount(() => {
    void (async () => {
      try {
        if (!root) throw "No library is open.";
        const [loaded, src] = await Promise.all([loadSheet(root, key), loadSource()]);
        name = src.name;
        sheet = { ...EMPTY_SHEET, ...loaded.sheet, entries: loaded.sheet.entries ?? [] };
        lastSaved = JSON.stringify($state.snapshot(sheet));
        warning = loaded.warning ?? "";
      } catch (e) {
        failed = String(e);
      } finally {
        loading = false;
        ready = true;
      }
    })();
  });

  async function loadSource(): Promise<{ name: string }> {
    if (kind === "case") {
      caseIds = await caseTracks(root, id);
      const c = cases.infos.find((x) => x.id === id) ?? session.library?.cases.find((x) => x.id === id);
      return { name: c?.name ?? "Case" };
    }
    // the open editor's document includes edits that are not saved yet: print what is on screen
    if (mixes.doc?.id === id) {
      mixDoc = mixes.doc;
      unsavedMix = mixes.dirty;
    } else {
      mixDoc = await mixOpen(root, id);
    }
    return { name: mixDoc.name };
  }

  // ---- saving the typed details ----------------------------------------------------------
  type SaveState = "idle" | "saving" | "saved" | "error";
  let saveState = $state<SaveState>("idle");
  let lastSaved = "";
  let timer: ReturnType<typeof setTimeout> | undefined;

  $effect(() => {
    const snap = JSON.stringify($state.snapshot(sheet));
    if (!ready) return;
    untrack(() => {
      if (snap === lastSaved) return;
      clearTimeout(timer);
      timer = setTimeout(() => void flush(), 700);
    });
  });

  // Saves run one after another: two overlapping invokes could land out of order and let an older
  // sheet overwrite a newer one. Each run re-reads the sheet when its turn comes.
  let chain: Promise<void> = Promise.resolve();

  function flush(): Promise<void> {
    clearTimeout(timer);
    timer = undefined;
    chain = chain.then(saveNow);
    return chain;
  }

  async function saveNow() {
    const snap = JSON.stringify($state.snapshot(sheet));
    if (!root || snap === lastSaved) return;
    saveState = "saving";
    try {
      await saveSheet(root, key, JSON.parse(snap));
      lastSaved = snap;
      saveState = "saved";
      warning = "";
    } catch (e) {
      saveState = "error";
      toast.show(`Could not save the setlist details: ${String(e)}`, "bad", 9000);
    }
  }

  function close() {
    if (timer !== undefined) void flush();
    onclose();
  }

  // remember the print options
  $effect(() => {
    JSON.stringify($state.snapshot(setlist.options));
    setlist.persist();
  });

  // ---- actions ---------------------------------------------------------------------------
  let frame = $state<HTMLIFrameElement>();

  function print() {
    try {
      const w = frame?.contentWindow;
      if (!w) throw new Error("the preview is not ready");
      w.focus();
      w.print();
    } catch (e) {
      toast.show(`Printing is not available here (${e instanceof Error ? e.message : String(e)}). Use Save as HTML and print that file from a browser.`, "bad", 10000);
    }
  }

  let saving = $state(false);
  async function saveHtml() {
    if (saving) return;
    saving = true;
    try {
      const dest = await pickSetlistPath(fileNameFor(sheet.title.trim() || name));
      if (!dest) return;
      const out = await saveSetlistHtml(root, dest, page(true));
      toast.show(`Saved ${out.path}`, "ok", 6000);
    } catch (e) {
      toast.show(`Could not save the page: ${String(e)}`, "bad", 9000);
    } finally {
      saving = false;
    }
  }

  const label = (r: { artist: string; title: string; missing: boolean; trackId: number }) =>
    r.missing ? `Track #${r.trackId} (no longer in the library)` : r.artist ? `${r.title} by ${r.artist}` : r.title;
  const editable = $derived(ready && !failed);
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && close()} />

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="scrim" onclick={close}>
  <div class="dlg" role="dialog" aria-modal="true" aria-label="Printable setlist" tabindex="-1" onclick={(e) => e.stopPropagation()}>
    <header>
      <h2>Printable setlist</h2>
      <span class="muted">{kind === "mix" ? "mix" : "case"} · {name || "…"}</span>
      <span class="grow"></span>
      <span class="status muted" role="status">
        {#if saveState === "saving"}Saving details…{:else if saveState === "saved"}Details saved{:else if saveState === "error"}Details not saved{/if}
      </span>
      <Button onclick={close}>Close</Button>
    </header>

    {#if loading}
      <p class="empty">Loading…</p>
    {:else if failed}
      <p class="empty bad">Could not open the setlist: {failed}</p>
    {:else}
      <div class="body">
        <div class="side">
          {#if warning}<p class="warn" role="alert">{warning}</p>{/if}
          {#each built.notes as n (n)}<p class="note-line">{n}</p>{/each}
          {#if unsavedMix}<p class="note-line">This mix has unsaved changes in the editor; the setlist shows them as they are on screen.</p>{/if}

          <section>
            <h3>Details</h3>
            <p class="muted small">Kept in this app on this computer. They are not written to the Pacemaker.</p>
            <label>Heading<input bind:value={sheet.title} maxlength="200" placeholder={name} /></label>
            <label>Event<input bind:value={sheet.event} maxlength="200" /></label>
            <label>Venue<input bind:value={sheet.venue} maxlength="200" /></label>
            <div class="two">
              <label>Date<input bind:value={sheet.date} maxlength="200" placeholder="free text" /></label>
              <label>DJ<input bind:value={sheet.dj} maxlength="200" /></label>
            </div>
            <label>Footer text<textarea rows="2" bind:value={sheet.footer} maxlength="1000"></textarea></label>
          </section>

          <section>
            <h3>Times</h3>
            <SetlistOptions part="time" />
            <label class:off={opts.timeMode !== "clock"}>Set starts at
              <input type="time" bind:value={sheet.start_clock} disabled={opts.timeMode !== "clock"} />
            </label>
            {#if opts.timeMode === "clock" && !sheet.start_clock}<p class="small muted">Enter a start time; until then the page shows time since the start.</p>{/if}
          </section>

          <section>
            <h3>Columns</h3>
            <SetlistOptions part="columns" {kind} {shown} />
          </section>

          <section>
            <h3>Page</h3>
            <SetlistOptions part="page" {kind} />
          </section>

          <section>
            <h3>Notes and energy</h3>
            <p class="muted small">
              The energy column shows the track's energy from the library ({energySource.source === "mine" ? "your rating" : energySource.source === "auto" ? "the estimate from the audio" : "your rating, else the estimate from the audio"}; see the Energy source setting) unless you pick a 1 to {MAX_ENERGY} value here, which is for this setlist only. The Pacemaker has no energy field.
              {#if repeats > 0}A track used more than once shares one note and rating.{/if}
            </p>
            {#if built.rows.length === 0}
              <p class="muted small">This {kind} has no tracks.</p>
            {:else}
              <ol class="rows">
                {#each built.rows as r, i (i)}
                  <li class:gone={r.missing}>
                    <span class="n muted">{i + 1}</span>
                    <span class="t" title={label(r)}>{label(r)}</span>
                    <select
                      aria-label="Energy of {label(r)}"
                      disabled={r.missing || !editable}
                      value={entryOf(sheet, r.trackId).energy}
                      onchange={(e) => (sheet = withEntry(sheet, r.trackId, { energy: Number(e.currentTarget.value) }))}
                    >
                      <option value={0}>{r.energy !== null ? `${r.energy} (library)` : "–"}</option>
                      {#each Array.from({ length: MAX_ENERGY }, (_, k) => k + 1) as n (n)}<option value={n}>{n}</option>{/each}
                    </select>
                    <input
                      aria-label="Note for {label(r)}"
                      placeholder="Note"
                      maxlength="300"
                      disabled={r.missing || !editable}
                      value={entryOf(sheet, r.trackId).note}
                      oninput={(e) => (sheet = withEntry(sheet, r.trackId, { note: e.currentTarget.value }))}
                    />
                  </li>
                {/each}
              </ol>
            {/if}
          </section>
        </div>

        <div class="main">
          <div class="bar">
            <Button variant="primary" onclick={print} disabled={built.rows.length === 0}>Print…</Button>
            <Button onclick={saveHtml} disabled={saving}>Save as HTML…</Button>
            <span class="muted small">Print opens the system print dialog; choose “Save as PDF” as the printer for a PDF. If printing does nothing on this system, use Save as HTML and print that file from a browser.</span>
          </div>
          <iframe bind:this={frame} title="Setlist preview" srcdoc={html} sandbox="allow-same-origin allow-modals"></iframe>
        </div>
      </div>
    {/if}
  </div>
</div>

<style>
  .scrim {
    position: fixed;
    inset: 0;
    z-index: 30;
    display: flex;
    align-items: center;
    justify-content: center;
    background: rgba(0, 0, 0, 0.6);
  }
  .dlg {
    display: flex;
    flex-direction: column;
    width: min(1240px, 96vw);
    height: min(860px, 94vh);
    background: var(--panel);
    border: 1px solid var(--line-hi);
    border-radius: 8px;
    outline: none;
  }
  header {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 10px 14px;
    border-bottom: 1px solid var(--line);
  }
  h2 {
    margin: 0;
    font-size: 16px;
  }
  .grow {
    flex: 1;
  }
  .status {
    font-size: 12px;
  }
  .empty {
    padding: 24px;
    margin: 0;
  }
  .bad {
    color: var(--bad-text);
  }
  .body {
    flex: 1;
    min-height: 0;
    display: grid;
    grid-template-columns: minmax(330px, 400px) 1fr;
  }
  .side {
    overflow: auto;
    padding: 4px 14px 14px;
    border-right: 1px solid var(--line);
  }
  .main {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .bar {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
    padding: 8px 12px;
    border-bottom: 1px solid var(--line);
  }
  iframe {
    flex: 1;
    min-height: 0;
    width: 100%;
    border: 0;
    background: var(--deep);
  }
  section {
    margin-top: 12px;
  }
  h3 {
    margin: 0 0 4px;
    font-size: 11px;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--muted);
  }
  label {
    display: grid;
    gap: 2px;
    margin-top: 6px;
    font-size: 12px;
    color: var(--muted);
  }
  label.off {
    opacity: 0.5;
  }
  .two {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 8px;
  }
  input:not([type="checkbox"]),
  select,
  textarea {
    width: 100%;
    box-sizing: border-box;
    min-width: 0;
  }
  textarea {
    background: var(--panel);
    color: var(--text);
    border: 1px solid var(--line);
    border-radius: 4px;
    padding: 5px 10px;
    font: inherit;
    resize: vertical;
  }
  .small {
    font-size: 12px;
    margin: 2px 0 0;
  }
  .warn {
    margin: 10px 0 0;
    padding: 6px 8px;
    border-radius: 4px;
    background: var(--bad-bg);
    color: var(--bad-text);
    font-size: 12px;
  }
  .note-line {
    margin: 10px 0 0;
    padding: 6px 8px;
    border-radius: 4px;
    background: var(--accent-soft);
    font-size: 12px;
  }
  .rows {
    list-style: none;
    margin: 6px 0 0;
    padding: 0;
    display: grid;
    gap: 4px;
  }
  .rows li {
    display: grid;
    grid-template-columns: 24px minmax(0, 1fr) 56px;
    grid-template-areas: "n t e" ". note note";
    gap: 2px 6px;
    align-items: center;
    padding: 4px 0;
    border-bottom: 1px solid var(--line);
  }
  .rows li .n {
    grid-area: n;
    text-align: right;
  }
  .rows li .t {
    grid-area: t;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .rows li select {
    grid-area: e;
  }
  .rows li input {
    grid-area: note;
  }
  .rows li.gone {
    color: var(--muted);
    font-style: italic;
  }
</style>
