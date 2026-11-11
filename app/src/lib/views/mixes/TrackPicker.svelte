<script lang="ts">
  // Searchable library list on the left of the editor. Drag rows onto a channel lane, double-click
  // a row, or use “Add at playhead”. The list is windowed, so a library of thousands stays cheap.
  import type { Track } from "$lib/api/types";
  import { mixes } from "$lib/state/mixes.svelte";
  import { session } from "$lib/state/session.svelte";
  import { addTracks } from "./actions";
  import { mmss } from "./layout";
  import { mv } from "./view.svelte";

  const ROW = 44;
  let query = $state("");
  let selected = $state.raw<number[]>([]);
  let scrollTop = $state(0);
  let boxH = $state(400);
  let anchor: number | null = null;

  const all = $derived(session.library?.tracks ?? []);
  const matches = $derived.by(() => {
    const words = query.toLowerCase().split(/\s+/).filter(Boolean);
    if (words.length === 0) return all;
    return all.filter((t) => {
      const hay = `${t.title} ${t.artist} ${t.album} ${t.genre} ${t.key} ${Math.round(t.bpm)}`.toLowerCase();
      return words.every((w) => hay.includes(w));
    });
  });
  const used = $derived.by(() => {
    const m = new Map<number, number>();
    for (const c of mixes.doc?.clips ?? []) m.set(c.trackId, (m.get(c.trackId) ?? 0) + 1);
    return m;
  });
  const first = $derived(Math.max(0, Math.floor(scrollTop / ROW) - 4));
  const last = $derived(Math.min(matches.length, Math.ceil((scrollTop + boxH) / ROW) + 4));
  const shown = $derived(matches.slice(first, last));

  function click(e: MouseEvent, t: Track, index: number) {
    if (e.shiftKey && anchor !== null) {
      const [a, b] = [Math.min(anchor, index), Math.max(anchor, index)];
      selected = matches.slice(a, b + 1).map((x) => x.track_id);
    } else if (e.ctrlKey || e.metaKey) {
      selected = selected.includes(t.track_id) ? selected.filter((x) => x !== t.track_id) : [...selected, t.track_id];
      anchor = index;
    } else {
      selected = [t.track_id];
      anchor = index;
    }
  }

  function dragstart(e: DragEvent, t: Track) {
    const ids = selected.includes(t.track_id) ? selected : [t.track_id];
    if (!selected.includes(t.track_id)) selected = [t.track_id];
    e.dataTransfer?.setData("application/x-pm-tracks", JSON.stringify(ids));
    e.dataTransfer?.setData("text/plain", ids.map((id) => all.find((x) => x.track_id === id)?.title ?? "").join(", "));
    if (e.dataTransfer) e.dataTransfer.effectAllowed = "copy";
  }

  function addSelected(at?: number) {
    const ids = selected.length ? selected : [];
    if (ids.length === 0) return;
    void addTracks(ids, at);
  }
</script>

<aside class="picker" aria-label="Library tracks">
  <div class="top">
    <input type="search" placeholder="Search title, artist, key, BPM…" bind:value={query} aria-label="Search the library" />
    <div class="count muted">{matches.length} of {all.length} track{all.length === 1 ? "" : "s"}</div>
  </div>
  <div class="list" bind:clientHeight={boxH} onscroll={(e) => (scrollTop = e.currentTarget.scrollTop)} role="listbox" aria-multiselectable="true" aria-label="Tracks">
    <div class="spacer" style:height="{matches.length * ROW}px">
      {#each shown as t, i (t.track_id)}
        {@const index = first + i}
        {@const bad = session.needs.get(t.track_id)}
        <div
          class="row"
          class:sel={selected.includes(t.track_id)}
          role="option"
          aria-selected={selected.includes(t.track_id)}
          tabindex="0"
          draggable="true"
          style:top="{index * ROW}px"
          style:height="{ROW}px"
          onclick={(e) => click(e, t, index)}
          ondblclick={() => {
            selected = [t.track_id];
            addSelected(mv.playhead);
          }}
          onkeydown={(e) => {
            if (e.key === "Enter") {
              selected = [t.track_id];
              addSelected(mv.playhead);
            }
          }}
          ondragstart={(e) => dragstart(e, t)}
        >
          <span class="t">{t.title}{#if used.get(t.track_id)}<em title="Already in this mix">×{used.get(t.track_id)}</em>{/if}</span>
          <span class="a">{t.artist}</span>
          <span class="m">
            <span class="num">{t.bpm ? t.bpm.toFixed(0) : "–"}</span>
            <span>{t.key || "–"}</span>
            <span class="num">{mmss(t.play_time_secs)}</span>
            {#if bad}<span class="flag" title="Needs analysis: {bad}">no grid</span>{/if}
          </span>
        </div>
      {/each}
    </div>
    {#if matches.length === 0}<div class="empty">No tracks match.</div>{/if}
  </div>
  <div class="foot">
    <button class="primary" disabled={selected.length === 0} onclick={() => addSelected(mv.playhead)}>Add at playhead</button>
    <button disabled={selected.length === 0} onclick={() => addSelected()}>Add to end</button>
  </div>
  <div class="muted tip">Drag a row onto a channel, or double-click to add at the playhead.</div>
</aside>

<style>
  .picker {
    width: 270px;
    flex: 0 0 270px;
    display: flex;
    flex-direction: column;
    min-height: 0;
    border-right: 1px solid var(--line);
    background: var(--panel);
  }
  .top {
    padding: 8px;
  }
  .top input {
    width: 100%;
    box-sizing: border-box;
  }
  .count {
    font-size: 11px;
    margin-top: 4px;
  }
  .list {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    position: relative;
    border-top: 1px solid var(--line);
  }
  .spacer {
    position: relative;
  }
  .row {
    position: absolute;
    left: 0;
    right: 0;
    box-sizing: border-box;
    padding: 4px 8px;
    border-bottom: 1px solid var(--line);
    cursor: grab;
    display: grid;
    grid-template-columns: 1fr;
    align-content: center;
    gap: 1px;
    font-size: 12px;
    outline: none;
  }
  .row:hover {
    background: var(--hover);
  }
  .row.sel {
    background: var(--accent-soft);
    box-shadow: inset 2px 0 0 var(--accent);
  }
  .row:focus-visible {
    background: var(--hover);
  }
  .t {
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    color: var(--text);
  }
  .t em {
    font-style: normal;
    color: var(--green);
    margin-left: 6px;
    font-size: 11px;
  }
  .a {
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    color: var(--muted);
    font-size: 11px;
    padding-right: 128px;
  }
  .m {
    position: absolute;
    right: 8px;
    bottom: 4px;
    display: flex;
    gap: 8px;
    font-size: 11px;
    color: var(--muted);
    background: inherit;
  }
  .num {
    font-variant-numeric: tabular-nums;
  }
  .flag {
    color: var(--bad);
  }
  .empty {
    padding: 14px;
    color: var(--muted);
  }
  .foot {
    display: flex;
    gap: 6px;
    padding: 8px;
    border-top: 1px solid var(--line);
  }
  .tip {
    font-size: 11px;
    padding: 0 8px 8px;
  }
</style>
