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
  }: {
    value: number | null;
    onset: (bpm: number) => void;
    id?: string;
    label?: string;
    min?: number;
    max?: number;
    placeholder?: string;
    disabled?: boolean;
  } = $props();

  const round = (v: number) => Math.round(v * 100) / 100;
  const shown = $derived(value === null ? "" : String(round(value)));

  function nudge(d: number) {
    const base = value ?? 0;
    const next = round(Math.min(max, Math.max(min, base + d)));
    if (next !== value) onset(next);
  }
</script>

<span class="bpm">
  <button type="button" {disabled} title="{label} down by 0.1 BPM" aria-label="{label} down 0.1" onclick={() => nudge(-0.1)}>−.1</button>
  <button type="button" {disabled} title="{label} down by 0.01 BPM" aria-label="{label} down 0.01" onclick={() => nudge(-0.01)}>−.01</button>
  <input
    {id}
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
  input {
    flex: 1 1 52px;
    width: 52px;
    min-width: 46px;
    box-sizing: border-box;
    text-align: center;
  }
  button {
    flex: none;
    padding: 0 3px;
    font-size: 10px;
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }
</style>
