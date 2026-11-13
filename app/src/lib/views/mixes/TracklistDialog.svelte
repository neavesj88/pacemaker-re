<script lang="ts">
  // Track list of the open mix with start times, in the formats people post with a mix: a text
  // list, CSV, YouTube chapters, a Mixcloud-style list and a CUE sheet. The backend computes the
  // times with the renderer's own clip timing, so a time here is a time in the exported audio, and
  // the text shown below is exactly the text that Copy puts on the clipboard and Save writes.
  import { save } from "@tauri-apps/plugin-dialog";
  import { exportTracklist, exportTracklistSave, type TracklistFormat, type TracklistOut } from "$lib/api/export";
  import { mixes } from "$lib/state/mixes.svelte";
  import { toast } from "$lib/state/toast.svelte";
  import { tracklist } from "$lib/state/tracklist.svelte";
  import { FORMATS, audioFileName, baseName, listFileName, withExtension } from "$lib/state/tracklistprefs";
  import Button from "$lib/ui/Button.svelte";
  import Modal from "$lib/ui/Modal.svelte";
  import { errText } from "./actions";
  import TracklistFields from "./TracklistFields.svelte";
  import { clock } from "./layout";

  const doc = $derived(mixes.doc);
  let out = $state.raw<TracklistOut | null>(null);
  let error = $state("");
  let loading = $state(false);
  let busy = $state(false);
  // the audio file the CUE sheet points at; starts as "<mix name>.ogg" and is the user's to change
  let cueAudio = $state("");
  let seq = 0;
  let nameFor = -1;

  $effect(() => {
    if (!tracklist.open) return;
    tracklist.ensureLoaded();
  });

  // closing the mix closes the dialog
  $effect(() => {
    if (!mixes.doc) tracklist.open = false;
  });

  // a new default audio name whenever the dialog opens for a different mix, or the mix is renamed
  // and the user has not typed their own
  let typed = false;
  $effect(() => {
    const d = mixes.doc;
    if (!tracklist.open || !d) return;
    if (nameFor !== d.id) {
      nameFor = d.id;
      typed = false;
    }
    if (!typed) cueAudio = audioFileName(d.name);
  });

  // (re)build the list whenever the dialog is open and the mix, the settings or the audio name change
  $effect(() => {
    if (!tracklist.open) return;
    const root = mixes.root;
    const d = mixes.doc;
    const options = tracklist.snapshot();
    const name = cueAudio;
    if (!root || !d) return;
    const mine = ++seq;
    loading = true;
    exportTracklist(root, d, options, name)
      .then((r) => {
        if (mine !== seq) return;
        out = r;
        error = "";
      })
      .catch((e) => {
        if (mine !== seq) return;
        out = null;
        error = errText(e);
      })
      .finally(() => {
        if (mine === seq) loading = false;
      });
  });

  const fmt = $derived(FORMATS.find((f) => f.id === tracklist.format) ?? FORMATS[0]);
  const text = $derived.by(() => {
    if (!out) return "";
    return { txt: out.txt, csv: out.csv, youtube: out.youtube, mixcloud: out.mixcloud, cue: out.cue }[tracklist.format];
  });
  const lines = $derived(text ? text.split(/\r?\n/).filter((l) => l.length > 0).length : 0);
  const blocked = $derived(tracklist.format === "cue" && !!out?.cueIssue);

  function pick(f: TracklistFormat) {
    tracklist.format = f;
    tracklist.save();
  }

  async function copy() {
    if (!text) return;
    try {
      await copyText(text);
      toast.show(`Copied the ${fmt.label.toLowerCase()} (${lines} line${lines === 1 ? "" : "s"})`, "ok");
    } catch (e) {
      toast.show(`Could not copy: ${errText(e)}`, "bad", 7000);
    }
  }

  /** The clipboard API, or a hidden textarea when the webview refuses it. */
  async function copyText(t: string) {
    try {
      await navigator.clipboard.writeText(t);
      return;
    } catch {
      // fall through to the old way
    }
    const el = document.createElement("textarea");
    el.value = t;
    el.setAttribute("readonly", "");
    el.style.position = "fixed";
    el.style.opacity = "0";
    document.body.appendChild(el);
    el.select();
    const ok = document.execCommand("copy");
    el.remove();
    if (!ok) throw new Error("the clipboard is not available");
  }

  async function saveFile() {
    const root = mixes.root;
    const d = mixes.doc;
    if (!root || !d || !out || busy || blocked) return;
    busy = true;
    try {
      const picked = await save({
        title: `Save the ${fmt.label.toLowerCase()}`,
        defaultPath: listFileName(d.name, tracklist.format),
        filters: [{ name: fmt.label, extensions: [fmt.ext] }],
      });
      if (!picked) return;
      const dest = withExtension(picked, fmt.ext);
      const written = await exportTracklistSave(root, d, tracklist.snapshot(), tracklist.format, dest, cueAudio);
      toast.show(`Saved ${baseName(written)}`, "ok");
    } catch (e) {
      toast.show(`Could not save the list: ${errText(e)}`, "bad", 10000);
    } finally {
      busy = false;
    }
  }

  function close() {
    seq++; // a build still running is ignored
    tracklist.open = false;
  }
</script>

{#if tracklist.open && doc}
  <div class="wide">
    <Modal title="Track list" onclose={close}>
      <div class="sub muted">
        {out ? `${out.entries.length} track${out.entries.length === 1 ? "" : "s"} · ${clock(out.durationSecs)} of audio` : "…"}
        · times are the times in the exported audio file (its first sound is 0:00)
      </div>

      {#if error}
        <div class="bad" role="alert">{error}</div>
      {:else if out}
        <div class="tablewrap">
          <table aria-label="Tracks and their start times">
            <thead><tr><th class="n">#</th><th class="t">Start</th><th>Track</th><th></th></tr></thead>
            <tbody>
              {#each out.entries as e (e.clipId)}
                <tr class:gone={!e.available}>
                  <td class="n num">{e.number}</td>
                  <td class="t num">{e.startLabel}</td>
                  <td class="name" title={e.album}>{e.artist ? `${e.artist} - ${e.title}` : e.title}</td>
                  <td class="flags muted">
                    {#if !e.available}no audio on the device{:else if e.overlapsPrevious}crossfade{/if}
                  </td>
                </tr>
              {:else}
                <tr><td colspan="4" class="muted empty-row">No track would be heard in the export, so there is nothing to list.</td></tr>
              {/each}
            </tbody>
          </table>
        </div>
        {#each out.warnings as w (w)}<div class="note-warn">{w}</div>{/each}
      {/if}

      <div class="tabs" role="tablist" aria-label="Format">
        {#each FORMATS as f (f.id)}
          <button role="tab" aria-selected={tracklist.format === f.id} class:on={tracklist.format === f.id} title={f.hint} onclick={() => pick(f.id)}>{f.label}</button>
        {/each}
      </div>

      {#if out}
        {#if tracklist.format === "youtube"}
          {#if out.youtubeValid}
            <div class="ok">YouTube will show these as chapters{out.youtubeIssues.length ? ", after the changes below" : ""}.</div>
          {:else}
            <div class="bad">YouTube will not show these as chapters.</div>
          {/if}
          {#each out.youtubeIssues as i (i)}<div class="note-warn">{i}</div>{/each}
        {:else if tracklist.format === "cue"}
          <div class="field">
            <label for="tl-audio">Audio file the sheet points at</label>
            <input
              id="tl-audio"
              type="text"
              bind:value={cueAudio}
              oninput={() => (typed = true)}
              spellcheck="false"
              autocomplete="off"
            />
          </div>
          {#if out.cueIssue}<div class="bad" role="alert">No CUE sheet: {out.cueIssue}.</div>{/if}
        {/if}

        <textarea class="preview" readonly rows="9" aria-label="{fmt.label} text" spellcheck="false" value={text}></textarea>
      {/if}

      <details class="settings">
        <summary>Settings</summary>
        <TracklistFields />
      </details>

      {#snippet actions()}
        <span class="busy muted">{loading ? "Working…" : ""}</span>
        <Button onclick={close} data-autofocus>Close</Button>
        <Button disabled={!text || blocked} onclick={copy} title="Copy the text above to the clipboard">Copy</Button>
        <Button variant="primary" disabled={!text || blocked || busy} onclick={saveFile} title="Save the text above as a .{fmt.ext} file">Save as file…</Button>
      {/snippet}
    </Modal>
  </div>
{/if}

<style>
  .wide :global(.modal) {
    width: 820px;
    max-height: 92vh;
    overflow: auto;
  }
  .sub {
    margin: -4px 0 8px;
    font-size: 12px;
  }
  .tablewrap {
    max-height: 190px;
    overflow: auto;
    border: 1px solid var(--line);
    border-radius: 4px;
  }
  table {
    width: 100%;
    border-collapse: collapse;
    font-size: 13px;
  }
  th {
    position: sticky;
    top: 0;
    background: var(--deep);
    text-align: left;
    font-weight: 600;
    color: var(--muted);
    padding: 4px 8px;
    font-size: 12px;
  }
  td {
    padding: 3px 8px;
    border-top: 1px solid var(--line);
  }
  .n {
    width: 34px;
  }
  .t {
    width: 72px;
  }
  td.n,
  td.t {
    text-align: right;
  }
  .name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 480px;
  }
  .flags {
    font-size: 12px;
    white-space: nowrap;
  }
  tr.gone .name {
    text-decoration: line-through;
    color: var(--muted);
  }
  .empty-row {
    padding: 14px 8px;
  }
  .tabs {
    display: flex;
    gap: 0;
    margin: 12px 0 8px;
  }
  .tabs button {
    background: var(--panel);
    color: var(--text);
    border: 1px solid var(--line);
    padding: 4px 12px;
    font: inherit;
    cursor: pointer;
  }
  .tabs button:first-child {
    border-radius: 4px 0 0 4px;
  }
  .tabs button:last-child {
    border-radius: 0 4px 4px 0;
  }
  .tabs button.on {
    background: var(--accent-soft);
    color: var(--accent);
    border-color: var(--accent);
  }
  .preview {
    width: 100%;
    box-sizing: border-box;
    background: var(--deep);
    color: var(--text);
    border: 1px solid var(--line);
    border-radius: 4px;
    padding: 8px;
    font: 12px/1.45 ui-monospace, Consolas, monospace;
    resize: vertical;
    white-space: pre;
  }
  .field {
    display: grid;
    gap: 4px;
    margin-bottom: 8px;
  }
  .field label {
    color: var(--muted);
    font-size: 12px;
  }
  .field input {
    width: 100%;
    box-sizing: border-box;
  }
  .ok {
    color: var(--ok);
    font-size: 12px;
    margin-bottom: 6px;
  }
  .bad,
  .note-warn {
    background: var(--bad-bg);
    color: var(--bad-text);
    border-radius: 4px;
    padding: 6px 8px;
    font-size: 12px;
    margin: 6px 0;
  }
  .settings {
    margin-top: 12px;
    font-size: 12px;
  }
  .settings summary {
    cursor: pointer;
    color: var(--muted);
  }
  .busy {
    margin-right: auto;
    align-self: center;
    font-size: 12px;
  }
</style>
