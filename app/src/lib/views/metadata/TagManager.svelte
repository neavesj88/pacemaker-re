<script lang="ts">
  // The tag manager: every tag in the library with its track count; rename, merge or delete one
  // across all tracks. Delete and merge show a confirmation that lists every track affected, and
  // the labels of removed tracks they also change. The lists come from the same helpers as the
  // counts, so what is shown is what is changed. "Forget" clears the labels of removed tracks.
  import { describeOrphan, normaliseTag, tagKey, type TrackLabels } from "$lib/state/labels";
  import { labelEditor, labels } from "$lib/state/labels.svelte";
  import { session } from "$lib/state/session.svelte";
  import { toast } from "$lib/state/toast.svelte";
  import Button from "$lib/ui/Button.svelte";
  import Modal from "$lib/ui/Modal.svelte";

  type Pending =
    | { kind: "delete"; tag: string; ids: number[]; gone: TrackLabels[] }
    | { kind: "merge"; from: string; into: string; ids: number[]; gone: TrackLabels[]; both: number }
    | { kind: "forget"; gone: TrackLabels[] };

  let query = $state("");
  let editing = $state<string | null>(null);
  let newName = $state("");
  let problem = $state("");
  let pending = $state<Pending | null>(null);
  let busy = $state(false);

  const shown = $derived(labels.vocabulary.filter((v) => !query.trim() || tagKey(v.tag).includes(tagKey(query.trim()))));
  const byId = $derived(new Map((session.library?.tracks ?? []).map((t) => [t.track_id, t])));
  const name = (id: number) => {
    const t = byId.get(id);
    return t ? [t.title, t.artist].filter(Boolean).join(" — ") || `Track ${id}` : `Track ${id}`;
  };
  const noun = (n: number) => `${n} track${n === 1 ? "" : "s"}`;
  const goneNoun = (n: number) => `${n} labelled track${n === 1 ? "" : "s"} no longer on this card`;

  function startRename(tag: string) {
    editing = tag;
    newName = tag;
    problem = "";
    pending = null;
  }

  async function submitRename() {
    if (editing === null || busy) return;
    const from = editing;
    const r = normaliseTag(newName);
    if (!r.ok) {
      problem = r.error;
      return;
    }
    if (r.tag === from) {
      editing = null;
      return;
    }
    const target = labels.vocabulary.find((v) => tagKey(v.tag) === tagKey(r.tag));
    const ids = labels.withTag(from);
    if (target && tagKey(target.tag) !== tagKey(from)) {
      // an existing, different tag: this is a merge, and it asks first
      const have = new Set(labels.withTag(target.tag));
      pending = { kind: "merge", from, into: target.tag, ids, gone: labels.orphansWithTag(from), both: ids.filter((id) => have.has(id)).length };
      return;
    }
    busy = true;
    const ok = await labels.rename(from, r.tag);
    busy = false;
    if (ok) {
      toast.show(`Renamed “${from}” to “${r.tag}” on ${noun(ids.length)}`, "ok");
      editing = null;
    }
  }

  async function confirm() {
    const p = pending;
    if (!p || busy) return;
    busy = true;
    const ok = p.kind === "delete" ? await labels.remove(p.tag) : p.kind === "merge" ? await labels.rename(p.from, p.into) : await labels.forgetOrphans();
    busy = false;
    if (!ok) return;
    const tail = (n: number) => (n ? ` and ${goneNoun(n)}` : "");
    toast.show(
      p.kind === "delete"
        ? `Deleted the tag “${p.tag}” from ${noun(p.ids.length)}${tail(p.gone.length)}`
        : p.kind === "merge"
          ? `Merged “${p.from}” into “${p.into}” on ${noun(p.ids.length)}${tail(p.gone.length)}`
          : `Forgot the labels of ${p.gone.length} removed track${p.gone.length === 1 ? "" : "s"}`,
      "ok",
    );
    pending = null;
    editing = null;
  }

  function askDelete(tag: string) {
    editing = null;
    pending = { kind: "delete", tag, ids: labels.withTag(tag), gone: labels.orphansWithTag(tag) };
  }

  function askForget() {
    editing = null;
    pending = { kind: "forget", gone: [...labels.orphanLabels] };
  }

  function onClose() {
    if (pending) pending = null;
    else if (editing !== null) editing = null;
    else labelEditor.closeManager();
  }
</script>

<Modal title="Manage tags" onclose={onClose}>
  {#if pending}
    <div class="confirm" role="alertdialog" aria-label={pending.kind === "delete" ? "Delete tag" : pending.kind === "merge" ? "Merge tags" : "Forget labels of removed tracks"}>
      {#if pending.kind === "delete"}
        <p>
          Delete the tag <b>“{pending.tag}”</b> from {noun(pending.ids.length)}{pending.gone.length ? ` and from ${goneNoun(pending.gone.length)}` : ""}? The
          tracks, their other tags, colours and energy stay as they are. This cannot be undone.
        </p>
      {:else if pending.kind === "merge"}
        <p>
          Merge <b>“{pending.from}”</b> into <b>“{pending.into}”</b>? {noun(pending.ids.length)}{pending.gone.length ? ` and ${goneNoun(pending.gone.length)}` : ""} will carry
          “{pending.into}” instead of “{pending.from}”{pending.both
            ? `; ${pending.both === 1 ? "1 already has" : `${pending.both} already have`} both and will end up with one`
            : ""}. “{pending.from}” will no longer exist. This cannot be undone.
        </p>
      {:else}
        <p>
          Forget the labels of {pending.gone.length} track{pending.gone.length === 1 ? "" : "s"} that are no longer on this card? Their tags, colours and
          energy are removed from the tag file. Tracks on this card are not touched. This cannot be undone.
        </p>
      {/if}
      {#if pending.kind !== "forget"}
        <div class="tracks">
          <div class="muted">{pending.kind === "delete" ? "Tracks losing the tag" : "Tracks changed"} ({pending.ids.length}):</div>
          <ul>
            {#each pending.ids as id (id)}<li>{name(id)}</li>{/each}
          </ul>
        </div>
      {/if}
      {#if pending.gone.length}
        <div class="tracks">
          <div class="muted">
            {pending.kind === "forget" ? "Labels to forget" : "Labels of tracks no longer on this card, also changed"} ({pending.gone.length}):
          </div>
          <ul>
            {#each pending.gone as g, i (i)}<li>{describeOrphan(g)}</li>{/each}
          </ul>
        </div>
      {/if}
      <div class="btns">
        <Button data-autofocus onclick={() => (pending = null)} disabled={busy}>Cancel</Button>
        <Button variant="primary" onclick={confirm} disabled={busy}>
          {busy
            ? "Working…"
            : pending.kind === "delete"
              ? `Delete tag from ${noun(pending.ids.length)}${pending.gone.length ? ` and ${pending.gone.length} removed` : ""}`
              : pending.kind === "merge"
                ? `Merge on ${noun(pending.ids.length)}${pending.gone.length ? ` and ${pending.gone.length} removed` : ""}`
                : `Forget ${pending.gone.length} label set${pending.gone.length === 1 ? "" : "s"}`}
        </Button>
      </div>
    </div>
  {:else}
    <p class="sub">Tags are kept by this app and do not appear on the Pacemaker. Renaming a tag to one that exists merges them.</p>
    <input class="find" type="search" aria-label="Find a tag" placeholder="Find a tag" bind:value={query} />
    <ul class="tags" aria-label="Tags in this library">
      {#each shown as v (tagKey(v.tag))}
        <li>
          {#if editing === v.tag}
            <form
              class="edit"
              onsubmit={(e) => {
                e.preventDefault();
                void submitRename();
              }}
            >
              <input aria-label="New name for {v.tag}" bind:value={newName} data-autofocus oninput={() => (problem = "")} />
              <Button small variant="primary" type="submit" disabled={busy}>Rename</Button>
              <Button small type="button" onclick={() => (editing = null)}>Cancel</Button>
            </form>
            {#if problem}<p class="problem" role="alert">{problem}</p>{/if}
          {:else}
            <span class="t">{v.tag}</span>
            <span class="muted n">{noun(v.count)}</span>
            <Button small onclick={() => startRename(v.tag)} aria-label="Rename {v.tag}">Rename</Button>
            <Button small onclick={() => askDelete(v.tag)} aria-label="Delete {v.tag}">Delete…</Button>
          {/if}
        </li>
      {:else}
        <li class="muted">{labels.vocabulary.length ? "No tag matches." : "No tags yet. Add some from a track's Tags, colour and energy dialog."}</li>
      {/each}
    </ul>
    {#if labels.orphans > 0}
      <p class="muted foot">
        The tag file also holds labels for {labels.orphans} track{labels.orphans === 1 ? "" : "s"} that are not on this card
        (removed or added again). They are kept and hidden, and Delete and Merge change them too.
        <Button small onclick={askForget}>Forget labels of removed tracks…</Button>
      </p>
    {/if}
    {#if labels.notice}<p class="problem" role="alert">{labels.notice}</p>{/if}
  {/if}

  {#snippet actions()}
    <Button onclick={() => labelEditor.closeManager()}>Close</Button>
  {/snippet}
</Modal>

<style>
  .sub {
    margin: 0 0 8px;
    color: var(--muted);
    font-size: 12px;
  }
  .find {
    width: 100%;
    box-sizing: border-box;
    margin-bottom: 8px;
  }
  .tags {
    margin: 0;
    padding: 0;
    list-style: none;
    max-height: 46vh;
    overflow: auto;
    border-top: 1px solid var(--line);
  }
  .tags li {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
    padding: 5px 4px;
    border-bottom: 1px solid var(--line);
  }
  .t {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .n {
    font-variant-numeric: tabular-nums;
    font-size: 12px;
  }
  .edit {
    display: flex;
    gap: 6px;
    flex: 1;
  }
  .edit input {
    flex: 1;
    min-width: 0;
  }
  .problem {
    flex-basis: 100%;
    margin: 2px 0 0;
    padding: 4px 8px;
    background: var(--bad-bg);
    color: var(--bad-text);
    border-radius: 4px;
    font-size: 12px;
  }
  .foot {
    font-size: 12px;
    margin: 8px 0 0;
  }
  .confirm p {
    margin: 0 0 8px;
  }
  .tracks ul {
    max-height: 36vh;
    overflow: auto;
    margin: 4px 0 0;
    padding: 0 0 0 18px;
    font-size: 12px;
  }
  .btns {
    display: flex;
    gap: 8px;
    justify-content: flex-end;
    margin-top: 12px;
  }
</style>
