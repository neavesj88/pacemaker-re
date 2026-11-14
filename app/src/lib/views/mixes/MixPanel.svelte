<script lang="ts">
  // Inspector when nothing is selected: the mix itself (name, reference tempo, library row).
  import type { MixMeta } from "$lib/api/mixes";
  import { mixes } from "$lib/state/mixes.svelte";
  import { session } from "$lib/state/session.svelte";
  import { toast } from "$lib/state/toast.svelte";
  import Stars from "$lib/ui/Stars.svelte";
  import { errText } from "./actions";
  import BpmField from "./BpmField.svelte";
  import { confirm } from "./confirm.svelte";
  import { clock } from "./layout";
  import { listMeta } from "./listmeta.svelte";
  import { toolPrefs } from "./listPrefs.svelte";
  import { mv } from "./view.svelte";

  const doc = $derived(mixes.doc!);
  // The library row lives in the list cache, not in the document: undo and redo restore old
  // snapshots of the document and must never bring back an old rating, genre or comments.
  const meta = $derived(listMeta.byId.get(doc.id) ?? doc.meta);

  async function setMeta(change: Partial<MixMeta>) {
    if (!meta) return;
    try {
      await listMeta.update(doc.id, change);
    } catch (e) {
      toast.show(`Could not save the details: ${errText(e)}`, "bad", 7000);
    }
  }
</script>

<h3>{doc.name}</h3>
<div class="muted">{doc.clips.length} track{doc.clips.length === 1 ? "" : "s"} · {clock(doc.lengthSec)}</div>

<div class="field">
  <label for="mx-name">Name</label>
  <input
    id="mx-name"
    type="text"
    maxlength="255"
    value={doc.name}
    onchange={(e) => {
      const v = e.currentTarget.value.trim();
      if (v) mixes.setName(v);
      else e.currentTarget.value = doc.name;
    }}
  />
</div>
<div class="field">
  <label for="mx-tempo">Mix tempo</label>
  <BpmField id="mx-tempo" value={doc.tempoBpm} onset={(v) => mixes.setTempo(v)} label="Mix tempo" placeholder="first track" stack />
</div>
<div class="muted small">The tempo tracks are matched to when added or synced (or double or half of it). Not stored in the mix file.</div>
{#if meta}
  <div class="field">
    <span class="lbl">Rating</span>
    <Stars rating={meta.rating} onchange={(r) => setMeta({ rating: r })} />
  </div>
  <div class="field">
    <label for="mx-genre">Genre</label>
    <input id="mx-genre" type="text" value={meta.genre} onchange={(e) => setMeta({ genre: e.currentTarget.value })} />
  </div>
  <div class="field">
    <label for="mx-comments">Comments</label>
    <input id="mx-comments" type="text" value={meta.comments} onchange={(e) => setMeta({ comments: e.currentTarget.value })} />
  </div>
  <div class="muted small">Rating, genre and comments are written to the library as soon as you change them.</div>
{/if}

<h4>Tool menu</h4>
<label class="check">
  <input type="checkbox" checked={toolPrefs.showTrans} onchange={(e) => toolPrefs.setShowTrans(e.currentTarget.checked)} />
  Offer the Trans tool when adding a tool
</label>
<div class="muted small">Off by default. Trans (a beat-synced gate) plays in the preview and the audio export, but its file layout has not been checked on a real Pacemaker, so a mix saved with it may be refused or played differently by {session.isComputer ? "the Pacemaker" : "the device"}. Turning this on is your choice. It only hides the menu entry: mixes that already have the tool still show it. A volume curve is the clip's Gain curve; there is no separate Volume tool.</div>

{#if mixes.problems.length > 0}
  <h4>Cannot save yet</h4>
  <div class="warn">
    <ul>
      {#each mixes.problems as p (p)}<li>{p}</li>{/each}
    </ul>
  </div>
{:else if mixes.dirty}
  <div class="muted small" style="margin-top: 10px">Unsaved changes. Ctrl+S saves.</div>
{/if}

<div class="buttons">
  <button onclick={() => mv.fit(doc.length)}>Fit mix to window</button>
  {#if mixes.dirty}<button
      onclick={() =>
        confirm.ask({
          title: "Revert changes",
          message: `Throw away every unsaved change to "${doc.name}" and reload it from the library?`,
          items: [`${mixes.undoDepth} edit${mixes.undoDepth === 1 ? "" : "s"} since the last save`],
          confirmLabel: "Revert",
          danger: true,
          run: () => mixes.revert(),
        })}>Revert changes…</button
    >{/if}
</div>

<p class="note">
  Click a clip, tool or crossfade to edit it. Drag clips along the bar; they snap to the beat grids, and clips that overlap are shown one above the other with the crossfade drawn on the overlap.
  Click the curve of a selected crossfade to add a node; right-click a node to remove it.
</p>

<style>
  .small {
    font-size: 12px;
    margin-top: 6px;
  }
  .lbl {
    color: var(--muted);
    font-size: 12px;
  }
  .check {
    display: flex;
    gap: 6px;
    align-items: center;
    color: var(--text);
    font-size: 13px;
  }
  ul {
    margin: 0;
    padding-left: 16px;
  }
</style>
