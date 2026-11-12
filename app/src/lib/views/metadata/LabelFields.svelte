<script lang="ts">
  // The tag, colour and energy fields for a set of tracks (one or many). Nothing is written from
  // here: choices are queued in `draft` and the parent saves them (TagEditor, or the metadata
  // editor's Save). A tag chip shows how many of the tracks carry it; a change that has been
  // queued but not saved is marked in words ("will be added", "will be removed").
  import {
    LABEL_COLOURS,
    ENERGY_LEVELS,
    NO_LABELS,
    canonicalTag,
    colourName,
    commonValue,
    draftAddTag,
    draftForgetTag,
    describeChanges,
    draftRemoveTag,
    parseTagInput,
    suggestTags,
    tagKey,
    tagPresence,
    type LabelColour,
    type LabelDraft,
  } from "$lib/state/labels";
  import { labels } from "$lib/state/labels.svelte";
  import ColourSwatch from "../tracks/ColourSwatch.svelte";

  let { ids, draft = $bindable(), disabled = false }: { ids: number[]; draft: LabelDraft; disabled?: boolean } = $props();

  const targets = $derived(ids.map((id) => labels.get(id) ?? NO_LABELS));
  const n = $derived(targets.length);
  const lines = $derived(describeChanges(draft, targets));
  const presence = $derived(tagPresence(targets));
  const removing = $derived(new Set(draft.remove.map(tagKey)));
  const adding = $derived(new Set(draft.add.map(tagKey)));
  const onAll = $derived(new Set(presence.filter((p) => p.have === n).map((p) => tagKey(p.tag))));

  const colourNow = $derived(commonValue(targets.map((t) => t.colour)));
  const energyNow = $derived(commonValue(targets.map((t) => t.energy)));
  // what each row shows: the queued choice, else what the tracks have
  const colourShown = $derived(draft.colour !== undefined ? ({ kind: "same", value: draft.colour } as const) : colourNow);
  const energyShown = $derived(draft.energy !== undefined ? ({ kind: "same", value: draft.energy } as const) : energyNow);

  function pickColour(c: LabelColour | null) {
    // choosing what every track already has is "no change"
    draft.colour = colourNow.kind === "same" && colourNow.value === c ? undefined : c;
  }
  function pickEnergy(e: number | null) {
    draft.energy = energyNow.kind === "same" && energyNow.value === e ? undefined : e;
  }

  // ---- tag entry with suggestions ----
  let typed = $state("");
  let problem = $state("");
  let active = $state(-1);
  let focused = $state(false);

  const lastPiece = $derived(typed.split(/[,;\n]/).at(-1) ?? "");
  const suggestions = $derived(
    focused && lastPiece.trim()
      ? suggestTags(labels.vocabulary, lastPiece, new Set([...onAll, ...adding].filter((k) => !removing.has(k))), 6)
      : [],
  );

  function addTyped(text: string) {
    const { tags, errors } = parseTagInput(text);
    problem = errors[0] ?? "";
    for (const t of tags) {
      const tag = canonicalTag(labels.vocabulary, t);
      const k = tagKey(tag);
      // already on every track and not queued for removal: nothing to add
      if (onAll.has(k) && !removing.has(k)) continue;
      draftAddTag(draft, tag);
    }
  }

  function commit() {
    if (!typed.trim()) return;
    addTyped(typed);
    typed = "";
    active = -1;
  }

  /** A suggestion chosen: it replaces the piece being typed (earlier comma-separated pieces are added too). */
  function pick(tag: string) {
    const before = typed.slice(0, typed.length - lastPiece.length);
    addTyped(before);
    addTyped(tag);
    typed = "";
    active = -1;
  }

  function onkeydown(e: KeyboardEvent) {
    if (e.key === "ArrowDown" && suggestions.length) {
      e.preventDefault();
      active = (active + 1) % suggestions.length;
    } else if (e.key === "ArrowUp" && suggestions.length) {
      e.preventDefault();
      active = (active - 1 + suggestions.length) % suggestions.length;
    } else if (e.key === "Enter" || e.key === ",") {
      e.preventDefault();
      e.stopPropagation();
      if (e.key === "Enter" && active >= 0 && suggestions[active]) pick(suggestions[active].tag);
      else commit();
    } else if (e.key === "Escape" && (typed || suggestions.length)) {
      e.preventDefault();
      e.stopPropagation();
      typed = "";
      active = -1;
    } else if (e.key === "Backspace" && !typed && draft.add.length) {
      draft.add = draft.add.slice(0, -1);
    }
  }
</script>

<div class="fields" class:off={disabled}>
  <div class="row">
    <span class="lbl" id="lf-colour">Colour</span>
    <div class="sws" role="group" aria-labelledby="lf-colour">
      {#each LABEL_COLOURS as c (c)}
        {@const on = colourShown.kind === "same" && colourShown.value === c}
        <button type="button" class="sw" class:on {disabled} aria-pressed={on} aria-label={colourName(c)} title={colourName(c)} onclick={() => pickColour(c)}>
          <ColourSwatch colour={c} size={16} />
        </button>
      {/each}
      <button type="button" class="txt" class:on={colourShown.kind === "same" && colourShown.value === null} {disabled} aria-pressed={colourShown.kind === "same" && colourShown.value === null} onclick={() => pickColour(null)}>None</button>
      {#if draft.colour === undefined && colourNow.kind === "mixed" && n > 1}<span class="note">tracks differ</span>{/if}
      {#if draft.colour !== undefined}
        <button type="button" class="undo" onclick={() => (draft.colour = undefined)}>undo</button>
      {/if}
    </div>
  </div>

  <div class="row">
    <span class="lbl" id="lf-energy">Energy</span>
    <div class="en" role="group" aria-labelledby="lf-energy">
      {#each ENERGY_LEVELS as e (e)}
        {@const on = energyShown.kind === "same" && energyShown.value === e}
        <button type="button" class="num-btn" class:on {disabled} aria-pressed={on} title="Energy {e} of 10" onclick={() => pickEnergy(e)}>{e}</button>
      {/each}
      <button type="button" class="txt" class:on={energyShown.kind === "same" && energyShown.value === null} {disabled} aria-pressed={energyShown.kind === "same" && energyShown.value === null} onclick={() => pickEnergy(null)}>None</button>
      {#if draft.energy === undefined && energyNow.kind === "mixed" && n > 1}<span class="note">tracks differ</span>{/if}
      {#if draft.energy !== undefined}
        <button type="button" class="undo" onclick={() => (draft.energy = undefined)}>undo</button>
      {/if}
    </div>
  </div>

  <div class="row tags">
    <span class="lbl" id="lf-tags">Tags</span>
    <div class="tagbox">
      <ul class="chips" aria-labelledby="lf-tags">
        {#each presence as p (tagKey(p.tag))}
          {@const gone = removing.has(tagKey(p.tag))}
          <li class="chip" class:gone class:part={p.have < n && !gone}>
            <span class="t">{p.tag}</span>
            {#if n > 1}<span class="have" title="{p.have} of {n} tracks have this tag">{p.have}/{n}</span>{/if}
            {#if gone}
              <span class="mark">will be removed</span>
              <button type="button" class="x" {disabled} aria-label="Keep the tag {p.tag}" title="Keep this tag" onclick={() => draftForgetTag(draft, p.tag)}>↺</button>
            {:else}
              <button type="button" class="x" {disabled} aria-label="Remove the tag {p.tag}" title={n > 1 ? "Remove this tag from every selected track" : "Remove this tag"} onclick={() => draftRemoveTag(draft, p.tag)}>×</button>
            {/if}
          </li>
        {/each}
        {#each draft.add as t (tagKey(t))}
          {#if !onAll.has(tagKey(t)) || removing.has(tagKey(t))}
            <li class="chip add">
              <span class="t">{t}</span>
              <span class="mark">will be added</span>
              <button type="button" class="x" {disabled} aria-label="Do not add the tag {t}" title="Do not add this tag" onclick={() => draftForgetTag(draft, t)}>×</button>
            </li>
          {/if}
        {/each}
        {#if presence.length === 0 && draft.add.length === 0}<li class="none">No tags yet</li>{/if}
      </ul>
      <div class="entry">
        <input
          type="text"
          role="combobox"
          aria-expanded={suggestions.length > 0}
          aria-controls="lf-sugg"
          aria-autocomplete="list"
          aria-label="Add a tag"
          data-own-enter
          autocomplete="off"
          spellcheck="false"
          placeholder="Add a tag, then Enter (commas add several)"
          {disabled}
          bind:value={typed}
          oninput={() => ((problem = ""), (active = -1))}
          onfocus={() => (focused = true)}
          onblur={() => setTimeout(() => (focused = false), 120)}
          {onkeydown}
        />
        {#if suggestions.length}
          <ul class="sugg" id="lf-sugg" role="listbox" aria-label="Tags in use">
            {#each suggestions as s, i (s.tag)}
              <!-- mouse only: the keyboard path is the arrow keys and Enter in the input -->
              <!-- svelte-ignore a11y_click_events_have_key_events -->
              <li role="option" aria-selected={i === active} class:act={i === active} onmousedown={(e) => (e.preventDefault(), pick(s.tag))}>
                <span>{s.tag}</span><span class="muted">{s.count}</span>
              </li>
            {/each}
          </ul>
        {/if}
      </div>
      {#if problem}<p class="problem" role="alert">{problem}</p>{/if}
    </div>
  </div>

  {#if lines.length}
    <div class="changes" aria-live="polite">
      <div class="head">Saving will:</div>
      <ul>
        {#each lines as l (l)}<li>{l}</li>{/each}
      </ul>
    </div>
  {/if}
</div>

<style>
  .fields.off {
    opacity: 0.6;
  }
  .row {
    display: grid;
    grid-template-columns: 64px 1fr;
    gap: 8px;
    align-items: center;
    margin-top: 10px;
  }
  .row.tags {
    align-items: start;
  }
  .lbl {
    font-size: 12px;
    color: var(--muted);
  }
  .sws,
  .en {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 4px;
  }
  button {
    font: inherit;
    color: var(--text);
    cursor: pointer;
  }
  button:disabled {
    cursor: default;
  }
  .sw,
  .num-btn,
  .txt {
    background: var(--deep);
    border: 1px solid var(--line);
    border-radius: 4px;
    padding: 2px;
  }
  .sw {
    display: inline-flex;
  }
  .num-btn {
    min-width: 24px;
    padding: 2px 4px;
    font-variant-numeric: tabular-nums;
  }
  .txt {
    padding: 2px 8px;
    font-size: 12px;
  }
  .sw:hover:not(:disabled),
  .num-btn:hover:not(:disabled),
  .txt:hover:not(:disabled) {
    border-color: var(--line-hi);
  }
  .on {
    border-color: var(--accent);
    background: var(--accent-soft);
    color: var(--accent);
  }
  .note {
    font-size: 12px;
    color: var(--muted);
  }
  .undo {
    background: none;
    border: none;
    padding: 0 4px;
    font-size: 12px;
    color: var(--accent);
  }
  .tagbox {
    min-width: 0;
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 5px;
    margin: 0 0 6px;
    padding: 0;
    list-style: none;
  }
  .none {
    color: var(--muted);
    font-size: 12px;
  }
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 1px 4px 1px 8px;
    background: var(--deep);
    border: 1px solid var(--line-hi);
    border-radius: 10px;
    font-size: 12px;
  }
  .chip.part {
    border-style: dashed;
  }
  .chip.add {
    border-color: var(--green);
    color: var(--green);
  }
  .chip.gone {
    border-color: var(--bad);
    color: var(--bad-text);
  }
  .chip.gone .t {
    text-decoration: line-through;
  }
  .have,
  .mark {
    font-size: 11px;
    color: var(--muted);
  }
  .chip.add .mark {
    color: var(--green);
  }
  .chip.gone .mark {
    color: var(--bad-text);
  }
  .x {
    width: 18px;
    height: 18px;
    padding: 0;
    background: none;
    border: none;
    border-radius: 9px;
    color: var(--muted);
    line-height: 1;
  }
  .x:hover:not(:disabled) {
    background: var(--hover);
    color: var(--text);
  }
  .entry {
    position: relative;
  }
  input {
    width: 100%;
    box-sizing: border-box;
    padding: 5px 8px;
    background: var(--deep);
    color: var(--text);
    border: 1px solid var(--line);
    border-radius: 4px;
    font: inherit;
  }
  input:focus {
    outline: none;
    border-color: var(--accent);
  }
  input::placeholder {
    color: var(--muted);
  }
  .sugg {
    position: absolute;
    z-index: 5;
    left: 0;
    right: 0;
    top: calc(100% + 2px);
    margin: 0;
    padding: 3px;
    list-style: none;
    background: var(--panel);
    border: 1px solid var(--line-hi);
    border-radius: 6px;
  }
  .sugg li {
    display: flex;
    justify-content: space-between;
    gap: 10px;
    padding: 3px 8px;
    border-radius: 4px;
    cursor: pointer;
  }
  .sugg li:hover,
  .sugg li.act {
    background: var(--hover);
  }
  .changes {
    margin-top: 12px;
    padding: 8px 10px;
    background: var(--deep);
    border: 1px solid var(--line);
    border-radius: 4px;
    font-size: 12px;
  }
  .changes .head {
    color: var(--muted);
  }
  .changes ul {
    margin: 2px 0 0;
    padding-left: 18px;
  }
  .problem {
    margin: 6px 0 0;
    padding: 4px 8px;
    background: var(--bad-bg);
    color: var(--bad-text);
    border-radius: 4px;
    font-size: 12px;
  }
</style>
