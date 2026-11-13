<script lang="ts">
  import type { Rule } from "$lib/api/cases";
  import { cases } from "$lib/state/cases.svelte";
  import { session } from "$lib/state/session.svelte";
  import { ENERGY_LEVELS, LABEL_COLOURS, colourName, parseTagInput } from "$lib/state/labels";
  import { labels } from "$lib/state/labels.svelte";
  import { KINDS, NO_CASE, NO_COLOUR, TAG_MATCHES, TEXT_OPS, caseNameDrift, defaultCaseId, defaultRule, formatDuration, kindId, parseDuration, stampCaseNames } from "$lib/state/smartRules";
  import ColourSwatch from "../tracks/ColourSwatch.svelte";
  import Button from "$lib/ui/Button.svelte";

  // One rule (anything but a group) of a smart case: the type menu and that type's inputs.
  let { rule = $bindable(), onremove }: { rule: Rule; onremove: () => void } = $props();

  const uid = $props.id();
  const groups = [...new Set(KINDS.map((k) => k.group))];
  const firstCase = () => defaultCaseId(cases.infos);
  const formats = $derived([...new Set((session.library?.tracks ?? []).map((t) => t.format).filter(Boolean))].sort());

  // A deleted case stays selectable as "(missing)" so the rule can be seen and fixed, not dropped.
  const caseKnown = $derived.by(() => {
    const r = rule;
    return r.kind === "in_case" && cases.infos.some((c) => c.id === r.case_id);
  });

  // A smart case cannot read its own Pacemaker copy (it is rewritten from the rules), so that case
  // is not offered. A rule that already points at it is kept visible and flagged, not dropped.
  const ownCopy = $derived(cases.smart.find((s) => s.id === cases.smartOpen)?.device_case_id ?? null);
  const offered = $derived(cases.infos.filter((c) => c.id !== ownCopy || (rule.kind === "in_case" && c.id === rule.case_id)));
  const pointsAtOwnCopy = $derived(rule.kind === "in_case" && ownCopy !== null && rule.case_id === ownCopy);
  const drift = $derived(caseNameDrift(rule, cases.infos));
  function pickCase(id: number) {
    if (rule.kind !== "in_case") return;
    rule.case_id = id;
    rule.case_name = cases.infos.find((c) => c.id === id)?.name;
  }
  function acceptName() {
    if (drift && rule.kind === "in_case") rule.case_name = drift.now;
  }

  // Length boxes hold text like 3:30; the rule gets seconds, or NaN while the text is not a length.
  let minText = $state(rule.kind === "duration_range" ? formatDuration(rule.min_secs) : "");
  let maxText = $state(rule.kind === "duration_range" ? formatDuration(rule.max_secs) : "");
  $effect(() => {
    if (rule.kind !== "duration_range") return;
    rule.min_secs = parseDuration(minText);
    rule.max_secs = parseDuration(maxText);
  });
  // Tags are typed as text ("peak, warm up"); the rule gets the tags that parse.
  let tagText = $state(rule.kind === "tags" ? rule.tags.join(", ") : "");
  let tagProblem = $state("");
  $effect(() => {
    if (rule.kind !== "tags") return;
    const { tags, errors } = parseTagInput(tagText);
    rule.tags = tags;
    tagProblem = errors[0] ?? "";
  });
  function toggleColour(c: string) {
    if (rule.kind !== "colour") return;
    rule.colours = rule.colours.includes(c) ? rule.colours.filter((x) => x !== c) : [...rule.colours, c];
  }
  function retype(id: string) {
    rule = defaultRule(id, firstCase());
    stampCaseNames([rule], cases.infos);
    tagText = "";
    if (rule.kind === "duration_range") {
      minText = formatDuration(rule.min_secs);
      maxText = formatDuration(rule.max_secs);
    }
  }
</script>

<div class="rule">
  <select value={kindId(rule)} aria-label="Rule type" onchange={(e) => retype(e.currentTarget.value)}>
    {#each groups as g (g)}
      <optgroup label={g}>
        {#each KINDS.filter((k) => k.group === g) as k (k.id)}<option value={k.id}>{k.label}</option>{/each}
      </optgroup>
    {/each}
  </select>

  {#if rule.kind === "text"}
    <select bind:value={rule.op} aria-label="Comparison">
      {#each TEXT_OPS as o (o.op)}<option value={o.op}>{o.label}</option>{/each}
    </select>
    <input class="wide" bind:value={rule.text} placeholder="text" aria-label="Text to match" />
  {:else if rule.kind === "rating_min"}
    <select bind:value={rule.stars} aria-label="Stars">
      {#each [1, 2, 3, 4, 5] as n (n)}<option value={n}>{"★".repeat(n)}</option>{/each}
    </select>
  {:else if rule.kind === "favourite"}
    <select bind:value={rule.value} aria-label="In Favourites">
      <option value={true}>are in Favourites</option>
      <option value={false}>are not in Favourites</option>
    </select>
  {:else if rule.kind === "plays_min"}
    <input type="number" min="1" bind:value={rule.n} aria-label="Plays" /> <span class="muted">times</span>
  {:else if rule.kind === "plays_range"}
    <input type="number" min="0" placeholder="any" bind:value={rule.min} aria-label="Minimum plays" />
    <span class="muted">to</span>
    <input type="number" min="0" placeholder="any" bind:value={rule.max} aria-label="Maximum plays" />
    <span class="muted">times</span>
  {:else if rule.kind === "played_within"}
    <input type="number" min="1" bind:value={rule.days} aria-label="Days" /> <span class="muted">days</span>
  {:else if rule.kind === "last_played_before"}
    <input type="number" min="1" bind:value={rule.days} aria-label="Days" /> <span class="muted">days</span>
    <label class="check"><input type="checkbox" bind:checked={rule.include_never} /> also tracks never played</label>
  {:else if rule.kind === "added_within"}
    <input type="number" min="1" bind:value={rule.days} aria-label="Days" /> <span class="muted">days</span>
  {:else if rule.kind === "added_before"}
    <input type="number" min="1" bind:value={rule.days} aria-label="Days" /> <span class="muted">days ago</span>
  {:else if rule.kind === "bpm_range"}
    <input type="number" min="0" step="0.5" placeholder="any" bind:value={rule.min} aria-label="Minimum BPM" />
    <span class="muted">to</span>
    <input type="number" min="0" step="0.5" placeholder="any" bind:value={rule.max} aria-label="Maximum BPM" />
  {:else if rule.kind === "year_range"}
    <input type="number" min="1" step="1" placeholder="any" bind:value={rule.min} aria-label="Earliest year" />
    <span class="muted">to</span>
    <input type="number" min="1" step="1" placeholder="any" bind:value={rule.max} aria-label="Latest year" />
  {:else if rule.kind === "duration_range"}
    <input class="time" bind:value={minText} placeholder="any" aria-label="Shortest length" />
    <span class="muted">to</span>
    <input class="time" bind:value={maxText} placeholder="any" aria-label="Longest length" />
    <span class="muted">minutes:seconds</span>
  {:else if rule.kind === "key" || rule.kind === "key_compatible"}
    <input class="key" bind:value={rule.key} placeholder="8A or Am" aria-label="Key" />
    {#if rule.kind === "key_compatible"}<span class="muted">same, next to it on the wheel, or its relative</span>{/if}
  {:else if rule.kind === "format"}
    <input class="key" list={`${uid}-formats`} bind:value={rule.value} placeholder="MP3" aria-label="File format" />
    <datalist id={`${uid}-formats`}>{#each formats as f (f)}<option value={f}></option>{/each}</datalist>
  {:else if rule.kind === "in_case"}
    <select bind:value={rule.value} aria-label="In the case or not">
      <option value={true}>is in</option>
      <option value={false}>is not in</option>
    </select>
    <select value={rule.case_id} aria-label="Case" onchange={(e) => pickCase(Number(e.currentTarget.value))}>
      {#if rule.case_id === NO_CASE}<option value={NO_CASE}>Choose a case…</option>
      {:else if !caseKnown}<option value={rule.case_id}>(missing case {rule.case_id})</option>{/if}
      {#each offered as c (c.id)}<option value={c.id}>{c.name}</option>{/each}
    </select>
    {#if offered.length === 0}<span class="muted">there are no other cases yet: make one first</span>{/if}
    {#if pointsAtOwnCopy}<span class="warn" role="status">This is the Pacemaker copy of this smart case, which it cannot read. Pick another case.</span>{/if}
    {#if drift}
      <span class="warn" role="status">
        Saved for a case called “{drift.saved}”; the case with this number is “{drift.now}” here (renamed, or the rule came from another card).
        <Button small variant="ghost" onclick={acceptName}>Keep “{drift.now}”</Button>
      </span>
    {/if}
  {:else if rule.kind === "tags"}
    <select bind:value={rule.how} aria-label="Any, all or none of the tags">
      {#each TAG_MATCHES as m (m.how)}<option value={m.how}>{m.label}</option>{/each}
    </select>
    <input class="wide" list={`${uid}-tags`} bind:value={tagText} placeholder="tags, separated by commas" aria-label="Tags" />
    <datalist id={`${uid}-tags`}>{#each labels.vocabulary as v (v.tag)}<option value={v.tag}></option>{/each}</datalist>
    {#if tagProblem}<span class="warn" role="status">{tagProblem}</span>{/if}
  {:else if rule.kind === "colour"}
    <select bind:value={rule.value} aria-label="Is or is not">
      <option value={true}>is</option>
      <option value={false}>is not</option>
    </select>
    <span class="sws" role="group" aria-label="Colours">
      {#each LABEL_COLOURS as c (c)}
        {@const on = rule.colours.includes(c)}
        <button type="button" class="sw" class:on aria-pressed={on} aria-label={colourName(c)} title={colourName(c)} onclick={() => toggleColour(c)}><ColourSwatch colour={c} size={14} /></button>
      {/each}
      <button type="button" class="nocol" class:on={rule.colours.includes(NO_COLOUR)} aria-pressed={rule.colours.includes(NO_COLOUR)} onclick={() => toggleColour(NO_COLOUR)}>No colour</button>
    </span>
    <span class="muted">(any of the ones lit)</span>
  {:else if rule.kind === "energy_range"}
    <select value={rule.min === null ? "" : String(rule.min)} aria-label="Lowest energy" onchange={(e) => (rule.kind === "energy_range" && (rule.min = e.currentTarget.value === "" ? null : Number(e.currentTarget.value)))}>
      <option value="">any</option>
      {#each ENERGY_LEVELS as n (n)}<option value={String(n)}>{n}</option>{/each}
    </select>
    <span class="muted">to</span>
    <select value={rule.max === null ? "" : String(rule.max)} aria-label="Highest energy" onchange={(e) => (rule.kind === "energy_range" && (rule.max = e.currentTarget.value === "" ? null : Number(e.currentTarget.value)))}>
      <option value="">any</option>
      {#each ENERGY_LEVELS as n (n)}<option value={String(n)}>{n}</option>{/each}
    </select>
    <span class="muted">under the Energy source setting; tracks with no energy never match</span>
  {:else if rule.kind === "analysed"}
    <select bind:value={rule.value} aria-label="Analysed">
      <option value={true}>have been analysed</option>
      <option value={false}>have not been analysed</option>
    </select>
  {/if}

  <Button small variant="ghost" onclick={onremove} aria-label="Remove this rule">Remove</Button>
</div>

<style>
  .rule {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }
  .rule input[type="number"] {
    width: 72px;
  }
  .key {
    width: 100px;
  }
  .time {
    width: 70px;
  }
  .wide {
    min-width: 180px;
  }
  .warn {
    color: var(--bad-text);
  }
  .sws {
    display: inline-flex;
    align-items: center;
    gap: 3px;
  }
  .sw,
  .nocol {
    background: var(--deep);
    border: 1px solid var(--line);
    border-radius: 4px;
    padding: 2px;
    color: var(--text);
    font: inherit;
    cursor: pointer;
  }
  .nocol {
    padding: 2px 8px;
    font-size: 12px;
  }
  .sw {
    display: inline-flex;
  }
  .sw.on,
  .nocol.on {
    border-color: var(--accent);
    background: var(--accent-soft);
  }
  .check {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    color: var(--muted);
  }
</style>
