<script lang="ts">
  // A tempo (BPM) number field with fine nudge buttons: 0.01 and 0.1 either way. The number takes
  // two decimals. `onset` runs with the new value when it is typed or nudged.
  let {
    value,
    onset,
    id,
    label = "Tempo",
    min = 40,
    max = 250,
    placeholder = "",
    disabled = false,
    stack = false,
  }: {
    value: number | null;
    onset: (bpm: number) => void;
    id?: string;
    label?: string;
    min?: number;
    max?: number;
    placeholder?: string;
    disabled?: boolean;
    // Narrow places (the inspector): the number gets its own row, the four nudge buttons share the row under it.
    stack?: boolean;
  } = $props();

  const round = (v: number) => Math.round(v * 100) / 100;
  const shown = $derived(value === null ? "" : String(round(value)));

  function nudge(d: number) {
    const base = value ?? 0;
    const next = round(Math.min(max, Math.max(min, base + d)));
    if (next !== value) onset(next);
  }
</script>

<span class="bpm" class:stack>
  <button type="button" {disabled} title="{label} down by 0.1 BPM" aria-label="{label} down 0.1" onclick={() => nudge(-0.1)}>−.1</button>
  <button type="button" {disabled} title="{label} down by 0.01 BPM" aria-label="{label} down 0.01" onclick={() => nudge(-0.01)}>−.01</button>
  <input
    {id}
    class="bpm-in"
    type="number"
    {min}
    {max}
    step="0.01"
    {placeholder}
    {disabled}
    aria-label={label}
    value={shown}
    onchange={(e) => {
      const v = e.currentTarget.valueAsNumber;
      if (Number.isFinite(v) && v >= min && v <= max) onset(round(v));
      else e.currentTarget.value = shown;
    }}
  />
  <button type="button" {disabled} title="{label} up by 0.01 BPM" aria-label="{label} up 0.01" onclick={() => nudge(0.01)}>+.01</button>
  <button type="button" {disabled} title="{label} up by 0.1 BPM" aria-label="{label} up 0.1" onclick={() => nudge(0.1)}>+.1</button>
</span>

<style>
  .bpm {
    display: inline-flex;
    align-items: stretch;
    gap: 2px;
    min-width: 0;
  }
  /* base.css pads every input 10px a side and the inspector forces number fields to min-width 0, which
     left 124.00 about 30px of room: a real width and a thin side padding keep the digits whole. The class
     keeps this rule above both. */
  .bpm .bpm-in[type="number"] {
    flex: 1 1 56px;
    width: 56px;
    min-width: 56px;
    padding-inline: 3px;
    box-sizing: border-box;
    text-align: center;
    appearance: textfield; /* the nudge buttons do the stepping; the spinner arrows took the room the digits need */
  }
  .bpm .bpm-in::-webkit-inner-spin-button,
  .bpm .bpm-in::-webkit-outer-spin-button {
    appearance: none;
    margin: 0;
  }
  .bpm.stack {
    display: flex;
    flex-wrap: wrap;
  }
  .bpm.stack .bpm-in[type="number"] {
    order: -1;
    flex: 1 0 100%;
    width: auto;
  }
  .bpm.stack button {
    flex: 1 1 0;
    min-width: 0;
    padding: 2px 3px;
  }
  button {
    flex: none;
    padding: 0 3px;
    font-size: 10px;
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }
</style>
