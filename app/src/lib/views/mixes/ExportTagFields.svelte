<script lang="ts">
  // Shared by the export dialog and the Settings view: the same state (`mixExport.prefs`, kept on
  // this computer), so a change in one place is the change in the other.
  import { mixExport as X } from "$lib/state/mixexport.svelte";
  import { open as openFile } from "@tauri-apps/plugin-dialog";
  import { exportCoverCheck } from "$lib/api/export";
  import { TAG_LIMITS, tagNote, yearOf } from "$lib/state/exportprefs";
  import { baseName } from "$lib/state/tracklistprefs";
  import { toast } from "$lib/state/toast.svelte";
  import Button from "$lib/ui/Button.svelte";
  import { errText } from "./actions";

  /** The open mix's name, which is the title tag's default; null where no mix is open (Settings), so there is no title field. */
  let { mixTitle = null }: { mixTitle?: string | null } = $props();

  X.ensureLoaded();

  function changed() {
    X.save();
  }

  async function pickCover() {
    try {
      const f = await openFile({ title: "Choose the cover image", multiple: false, directory: false, filters: [{ name: "JPEG or PNG image", extensions: ["jpg", "jpeg", "png"] }] });
      if (typeof f === "string" && f) {
        X.prefs.coverPath = f;
        changed();
      }
    } catch (e) {
      toast.show(`Could not open the file dialog: ${errText(e)}`, "bad", 7000);
    }
  }

  // the remembered cover may have moved or been deleted since: say so now, with the sentence the
  // export would refuse with (the same check), not after the save window
  let coverProblem = $state("");
  $effect(() => {
    const path = X.prefs.writeTags ? X.prefs.coverPath.trim() : "";
    coverProblem = "";
    if (!path) return;
    let stale = false;
    exportCoverCheck(path)
      .then((m) => {
        if (!stale) coverProblem = m ?? "";
      })
      .catch(() => {});
    return () => (stale = true);
  });

  function removeCover() {
    X.prefs.coverPath = "";
    changed();
  }
</script>

<label class="check"><input type="checkbox" bind:checked={X.prefs.writeTags} onchange={changed} /> Write tags into the file</label>
{#if X.prefs.writeTags}
  <div class="grid">
    {#if mixTitle !== null}
      <label for="xt-title">Title</label>
      <input id="xt-title" type="text" maxlength={TAG_LIMITS.text} placeholder={mixTitle} bind:value={X.titleText} autocomplete="off" />
    {/if}
    <label for="xt-artist">Artist (your DJ name)</label>
    <input id="xt-artist" type="text" maxlength={TAG_LIMITS.text} bind:value={X.prefs.djName} onchange={changed} autocomplete="off" />
    <label for="xt-album">Album</label>
    <input id="xt-album" type="text" maxlength={TAG_LIMITS.text} bind:value={X.prefs.album} onchange={changed} autocomplete="off" />
    <label for="xt-year">Year</label>
    <input id="xt-year" class="num-in" type="text" inputmode="numeric" maxlength="4" placeholder={yearOf(X.prefs)} bind:value={X.prefs.year} onchange={changed} autocomplete="off" />
    <label for="xt-comment">Comment</label>
    <input id="xt-comment" type="text" maxlength={TAG_LIMITS.comment} bind:value={X.prefs.comment} onchange={changed} autocomplete="off" />
    <span class="lbl">Cover image</span>
    <span class="cover">
      <Button small onclick={() => void pickCover()}>{X.prefs.coverPath ? "Change…" : "Choose image…"}</Button>
      {#if X.prefs.coverPath}
        <span class="name" title={X.prefs.coverPath}>{baseName(X.prefs.coverPath)}</span>
        <Button small onclick={removeCover}>Remove</Button>
      {:else}
        <span class="muted small">JPEG or PNG, up to 8 MB</span>
      {/if}
    </span>
  </div>
  <label class="check"><input type="checkbox" bind:checked={X.prefs.chapters} onchange={changed} /> Put the track list in the file as chapters (the same list and times as the CUE sheet)</label>
  {#if coverProblem}<div class="note-warn" role="alert">{coverProblem}</div>{/if}
  <div class="muted small">{tagNote(X.prefs.format)}</div>
{/if}

<style>
  .grid {
    display: grid;
    grid-template-columns: max-content 1fr;
    gap: 6px 12px;
    align-items: center;
    margin: 8px 0;
  }
  .grid label,
  .grid .lbl {
    color: var(--muted);
  }
  .num-in {
    width: 78px;
  }
  .check {
    display: block;
    margin: 6px 0;
  }
  .small {
    font-size: 12px;
  }
  .note-warn {
    background: var(--bad-bg);
    color: var(--bad-text);
    border-radius: 4px;
    padding: 6px 8px;
    font-size: 12px;
    margin: 6px 0;
  }
  .grid input[type="text"] {
    width: 100%;
    min-width: 0;
  }
  .grid input.num-in[type="text"] {
    width: 78px;
  }
  .cover {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }
  .cover .name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 320px;
  }
</style>
