<script lang="ts">
  // The grid editor's track picker: a box that shows the open track and, on focus, a short list of
  // matches that narrows as you type. It replaced a <select> with an <option> per track, which took
  // seconds to open on a weak machine with a big library (see trackPick.ts).
  import { buildIndex, matchTracks, moveActive, trackLabel, type Index, type Pickable } from "./trackPick";

  let {
    tracks,
    current,
    onpick,
  }: {
    tracks: readonly Pickable[];
    /** The open track, or null. */
    current: number | null;
    onpick: (id: number) => void;
  } = $props();

  const shown = $derived(tracks.find((t) => t.track_id === current));
  let open = $state(false);
  let query = $state("");
  let active = $state(0);
  let index: Index | null = null;
  let input = $state<HTMLInputElement>();
  let list = $state<HTMLUListElement>();

  const matches = $derived.by(() => {
    if (!open) return { rows: [] as Pickable[], more: 0 };
    index ??= buildIndex(tracks);
    return matchTracks(index, query);
  });

  function show() {
    index = null; // the library may have changed since the last time
    query = "";
    const at = current === null ? -1 : tracks.findIndex((t) => t.track_id === current);
    active = at >= 0 && at < 100 ? at : 0;
    open = true;
  }

  function close() {
    open = false;
    query = "";
  }

  function pick(id: number) {
    close();
    input?.blur();
    onpick(id);
  }

  function onkeydown(e: KeyboardEvent) {
    if (!open && (e.key === "ArrowDown" || e.key === "ArrowUp")) {
      show();
      e.preventDefault();
      return;
    }
    if (!open) return;
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      active = moveActive(active, e.key === "ArrowDown" ? 1 : -1, matches.rows.length);
      e.preventDefault();
      list?.children[active]?.scrollIntoView({ block: "nearest" });
    } else if (e.key === "Enter") {
      const row = matches.rows[active];
      if (row) pick(row.track_id);
      e.preventDefault();
    } else if (e.key === "Escape") {
      close();
      input?.blur();
      e.preventDefault();
      e.stopPropagation();
    }
  }
</script>

<div class="pick">
  <input
    bind:this={input}
    type="text"
    role="combobox"
    aria-label="Track"
    aria-expanded={open}
    aria-controls="grid-track-list"
    aria-autocomplete="list"
    autocomplete="off"
    spellcheck="false"
    data-own-enter
    placeholder={open ? "Type to search the library" : "Choose a track…"}
    value={open ? query : shown ? trackLabel(shown) : ""}
    oninput={(e) => {
      query = e.currentTarget.value;
      active = 0;
    }}
    onfocus={(e) => {
      show();
      e.currentTarget.select();
    }}
    onclick={() => {
      if (!open) show();
    }}
    onblur={close}
    {onkeydown}
  />
  {#if open}
    <ul id="grid-track-list" role="listbox" aria-label="Tracks" bind:this={list}>
      {#each matches.rows as t, i (t.track_id)}
        <!-- mouse only: the keyboard path is the arrow keys and Enter in the box -->
        <!-- svelte-ignore a11y_click_events_have_key_events -->
        <li role="option" aria-selected={t.track_id === current} class:act={i === active} class:cur={t.track_id === current} onmousedown={(e) => (e.preventDefault(), pick(t.track_id))}>
          {trackLabel(t)}
        </li>
      {:else}
        <li class="none" role="presentation">No track matches</li>
      {/each}
      {#if matches.more > 0}<li class="none" role="presentation">{matches.more.toLocaleString()} more: type to narrow the list</li>{/if}
    </ul>
  {/if}
</div>

<style>
  .pick {
    position: relative;
    width: 380px;
    max-width: 100%;
  }
  input {
    width: 100%;
    box-sizing: border-box;
    text-overflow: ellipsis;
  }
  ul {
    position: absolute;
    z-index: 20;
    left: 0;
    right: 0;
    top: calc(100% + 2px);
    margin: 0;
    padding: 3px;
    list-style: none;
    max-height: min(360px, 55vh);
    overflow-y: auto;
    background: var(--panel);
    border: 1px solid var(--line-hi);
    border-radius: 6px;
  }
  li {
    padding: 3px 8px;
    border-radius: 4px;
    cursor: pointer;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  li:hover,
  li.act {
    background: var(--hover);
  }
  li.cur {
    color: var(--accent);
  }
  li.none {
    color: var(--muted);
    cursor: default;
    font-size: 12px;
  }
  li.none:hover {
    background: none;
  }
</style>
