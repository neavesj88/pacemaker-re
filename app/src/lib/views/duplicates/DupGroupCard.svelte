<script lang="ts">
  // One set of copies: why it is a set, every field that differs between the tracks (the fields
  // that read the same are folded into one line), a radio to choose the copy to keep and a box
  // to say a track is not a copy after all. Nothing is changed from here.
  import type { Group } from "$lib/api/duplicates";
  import { compareMembers, copiesOf, memberLinks, mergeable, plural, reasonLines, type Choice } from "$lib/state/dupeLogic";
  import { duplicates } from "$lib/state/duplicates.svelte";

  let { group, choice }: { group: Group; choice: Choice } = $props();

  const cmp = $derived(compareMembers(group.members));
  const reasons = $derived(reasonLines(group));
  const keeperRow = $derived(group.members.find((m) => m.track_id === choice.keeper));
  const name = $derived([keeperRow?.artist, keeperRow?.title].filter(Boolean).join(" - ") || keeperRow?.location || `track ${choice.keeper}`);
  const copies = $derived(copiesOf(group, choice).length);
  const ok = $derived(mergeable(group, choice));
  const cols = $derived(`150px repeat(${group.members.length}, minmax(170px, 1fr))`);
  const linkText = (id: number) => {
    const l = memberLinks(group, id);
    const audio = l.filter((x) => x.mode === "audio" && x.score !== null);
    return audio.length ? `sound ${Math.floor(Math.max(...audio.map((x) => x.score ?? 0)) * 100 + 1e-6)}% alike` : "";
  };
</script>

<section class="card" class:off={!choice.picked || !ok} aria-label={`Copies of ${name}`}>
  <header>
    <label class="pick" title={ok ? "Include this set in the merge" : "Nothing to merge in this set"}>
      <input type="checkbox" checked={choice.picked && ok} disabled={!ok} onchange={(e) => duplicates.setPicked(group.id, e.currentTarget.checked)} />
      <span class="nm">{name}</span>
    </label>
    <span class="muted why">{plural(group.members.length, "track")}: {reasons.join("; ")}</span>
    <span class="grow"></span>
    <span class="muted">{ok ? `${plural(copies, "copy", "copies")} to merge into the kept track` : "no copy left to merge"}</span>
  </header>

  <div class="scroll">
    <div class="grid" style:grid-template-columns={cols} role="table" aria-label="Fields that differ">
      <div class="cell lab" role="rowheader">Keep</div>
      {#each group.members as m (m.track_id)}
        {@const kept = m.track_id === choice.keeper}
        {@const out = choice.excluded.includes(m.track_id)}
        <div class="cell head" class:kept class:out role="columnheader">
          <label class="keep">
            <input type="radio" name={`keep-${group.id}`} checked={kept} onchange={() => duplicates.setKeeper(group.id, m.track_id)} />
            <b>{kept ? "Keep this one" : "Keep"}</b>
            <span class="muted id">#{m.track_id}</span>
          </label>
          <label class="not" class:dis={kept}>
            <input type="checkbox" checked={out} disabled={kept} onchange={(e) => duplicates.setExcluded(group.id, m.track_id, e.currentTarget.checked)} />
            not a copy of these
          </label>
          {#if m.track_id === group.suggested_keeper}<span class="tag">suggested</span>{/if}
          {#if linkText(m.track_id)}<span class="muted small">{linkText(m.track_id)}</span>{/if}
        </div>
      {/each}

      {#each cmp.differ as row (row.key)}
        <div class="cell lab" role="rowheader">{row.label}</div>
        {#each row.values as v, i (group.members[i].track_id)}
          {@const m = group.members[i]}
          <div class="cell val" class:kept={m.track_id === choice.keeper} class:out={choice.excluded.includes(m.track_id)} class:bad={row.key === "file" && m.file !== "present"} role="cell" title={v}>{v}</div>
        {/each}
      {/each}
    </div>
  </div>

  {#if cmp.same.length > 0}
    <details>
      <summary>The same in every track ({cmp.same.length} fields)</summary>
      <dl>
        {#each cmp.same as s (s.label)}
          <dt>{s.label}</dt>
          <dd>{s.value}</dd>
        {/each}
      </dl>
    </details>
  {/if}
</section>

<style>
  .card {
    margin: 0 0 12px;
    background: var(--panel);
    border: 1px solid var(--line);
    border-radius: 6px;
  }
  .card.off {
    opacity: 0.65;
  }
  header {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: 6px 14px;
    padding: 8px 12px;
    border-bottom: 1px solid var(--line);
  }
  .pick {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }
  .nm {
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 52ch;
  }
  .grow {
    flex: 1;
  }
  .scroll {
    overflow-x: auto;
  }
  .grid {
    display: grid;
    min-width: max-content;
    width: 100%;
  }
  .cell {
    padding: 4px 10px;
    border-bottom: 1px solid var(--line);
    min-width: 0;
  }
  .lab {
    color: var(--muted);
    background: var(--deep);
  }
  .head {
    display: flex;
    flex-direction: column;
    gap: 2px;
    background: var(--deep);
  }
  .head.kept {
    box-shadow: inset 0 2px 0 var(--accent);
  }
  .val {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .val.kept {
    background: var(--accent-soft);
  }
  .out {
    opacity: 0.5;
    text-decoration: line-through;
    text-decoration-color: var(--line-hi);
  }
  .head.out {
    text-decoration: none;
  }
  .bad {
    color: var(--bad-text);
  }
  .keep,
  .not {
    display: flex;
    align-items: center;
    gap: 6px;
    cursor: pointer;
  }
  .not {
    font-size: 11px;
    color: var(--muted);
  }
  .not.dis {
    visibility: hidden;
  }
  .id {
    margin-left: auto;
  }
  .tag {
    align-self: flex-start;
    padding: 0 6px;
    font-size: 10px;
    color: var(--text);
    border: 1px solid var(--line-hi);
    border-radius: 8px;
  }
  .small {
    font-size: 11px;
  }
  details {
    padding: 6px 12px 8px;
    font-size: 12px;
  }
  summary {
    color: var(--muted);
    cursor: pointer;
  }
  dl {
    display: grid;
    grid-template-columns: max-content 1fr;
    gap: 2px 14px;
    margin: 6px 0 0;
  }
  dt {
    color: var(--muted);
  }
  dd {
    margin: 0;
    overflow-wrap: anywhere;
  }
</style>
