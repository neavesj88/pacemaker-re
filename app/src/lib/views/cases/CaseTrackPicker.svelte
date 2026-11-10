<script lang="ts">
  import { mmss } from "$lib/format";
  import { session } from "$lib/state/session.svelte";
  import Button from "$lib/ui/Button.svelte";
  import Modal from "$lib/ui/Modal.svelte";

  // Picks library tracks to add to a case: search, tick, Add. Tracks already in the case are
  // listed greyed out and cannot be ticked.
  let {
    caseName,
    existing,
    onadd,
    onclose,
  }: { caseName: string; existing: Set<number>; onadd: (ids: number[]) => void; onclose: () => void } = $props();

  const LIMIT = 200;
  let query = $state("");
  let picked = $state<Set<number>>(new Set());

  let matches = $derived.by(() => {
    const q = query.trim().toLowerCase();
    const all = session.library?.tracks ?? [];
    return q ? all.filter((t) => [t.title, t.artist, t.album, t.genre, t.key].some((f) => f.toLowerCase().includes(q))) : all;
  });
  let shown = $derived(matches.slice(0, LIMIT));
  let pickable = $derived(matches.filter((t) => !existing.has(t.track_id)));

  function toggle(id: number) {
    const next = new Set(picked);
    if (!next.delete(id)) next.add(id);
    picked = next;
  }
</script>

<Modal title={`Add tracks to “${caseName}”`} {onclose}>
  <!-- svelte-ignore a11y_autofocus -->
  <input class="search" data-autofocus placeholder="Search title, artist, album, genre or key" bind:value={query} />
  <div class="tools muted">
    <span>{matches.length} track{matches.length === 1 ? "" : "s"}{matches.length > LIMIT ? `, first ${LIMIT} shown: narrow the search` : ""}</span>
    <Button small variant="link" onclick={() => (picked = new Set([...picked, ...pickable.map((t) => t.track_id)]))} disabled={pickable.length === 0}>
      Tick all {query.trim() ? "matches" : "tracks"}
    </Button>
    <Button small variant="link" onclick={() => (picked = new Set())} disabled={picked.size === 0}>Clear</Button>
  </div>
  <div class="list">
    {#each shown as t (t.track_id)}
      {@const inCase = existing.has(t.track_id)}
      <label class:dim={inCase}>
        <input type="checkbox" checked={inCase || picked.has(t.track_id)} disabled={inCase} onchange={() => toggle(t.track_id)} />
        <span class="title">{t.title}</span>
        <span class="muted">{t.artist}</span>
        <span class="muted num">{mmss(t.play_time_secs)}</span>
        {#if inCase}<span class="muted">in case</span>{/if}
      </label>
    {/each}
    {#if shown.length === 0}<p class="muted pad">No tracks match.</p>{/if}
  </div>
  {#snippet actions()}
    <Button onclick={onclose}>Cancel</Button>
    <Button
      variant="primary"
      disabled={picked.size === 0}
      onclick={() => {
        onadd([...picked]);
        onclose();
      }}
    >Add {picked.size} track{picked.size === 1 ? "" : "s"}</Button>
  {/snippet}
</Modal>

<style>
  .search {
    width: 100%;
    box-sizing: border-box;
  }
  .tools {
    display: flex;
    align-items: center;
    gap: 12px;
    margin: 6px 0;
  }
  .tools span {
    flex: 1;
  }
  .list {
    height: 320px;
    overflow: auto;
    background: var(--deep);
    border: 1px solid var(--line);
    border-radius: 4px;
  }
  label {
    display: grid;
    grid-template-columns: 20px 1fr 1fr 48px 52px;
    gap: 8px;
    align-items: center;
    padding: 3px 8px;
    cursor: pointer;
  }
  label:hover {
    background: var(--hover);
  }
  label.dim {
    opacity: 0.55;
    cursor: default;
  }
  label span {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  input[type="checkbox"] {
    padding: 0;
  }
  .pad {
    padding: 12px;
  }
</style>
