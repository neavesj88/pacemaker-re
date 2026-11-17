<script lang="ts">
  // The auto-transition settings: which recipe the Auto buttons build, how long, where a hard cut
  // falls, and what happens to tools of yours that are in the way. Remembered between sessions.
  import { LOOP_BEATS, LOOP_REPEATS, LOOP_SHORTEST, RECIPES, SECTION_REACH, lengthLabel, loopLengthLabel, nearestLength, recipeDef, requestedBars, sectionEdgeWords, type RecipeId } from "$lib/state/transitionRecipes";
  import type { CutAt, ConflictPolicy, CrossfadePlace, LoopOutStart, OnMove, SectionEdge } from "$lib/state/transitionRecipes";
  import { recipePrefs } from "./recipePrefs.svelte";

  const p = $derived(recipePrefs.value);
  const def = $derived(recipeDef(p.recipe));
  const effective = $derived(requestedBars(p.recipe, p));
  /** The loop lengths of a loop cut, longest first, as the settings will play them. */
  const cutSteps = $derived.by(() => {
    const out: number[] = [];
    for (let l = p.loopCutBeats; l >= p.loopCutShortest - 1e-9; l /= 2) out.push(l);
    return out;
  });
</script>

<div class="field">
  <label for="rs-recipe">Recipe</label>
  <select id="rs-recipe" value={p.recipe} onchange={(e) => recipePrefs.set({ recipe: e.currentTarget.value as RecipeId })}>
    {#each RECIPES as r (r.id)}<option value={r.id}>{r.label}</option>{/each}
  </select>
</div>
<div class="field">
  <label for="rs-bars">Auto crossfade length</label>
  <select id="rs-bars" value={p.bars == null ? "auto" : String(p.bars)} onchange={(e) => recipePrefs.set({ bars: e.currentTarget.value === "auto" ? null : Number(e.currentTarget.value) })}>
    <option value="auto">Each recipe's own</option>
    {#each [0.25, 0.5, 1, 2, 4, 8, 16, 32, 64] as n (n)}<option value={String(n)}>{lengthLabel(n)}</option>{/each}
  </select>
</div>
<div class="muted small">
  {def.label} will be {lengthLabel(effective)}{p.bars != null && nearestLength(p.recipe, p.bars) !== p.bars ? ` (offers ${def.lengths.map(lengthLabel).join(", ")})` : ""}. Never longer than the shared audio.
</div>
<div class="field">
  <label for="rs-place">Crossfade sits</label>
  <select id="rs-place" value={p.placeAt} onchange={(e) => recipePrefs.set({ placeAt: e.currentTarget.value as CrossfadePlace })}>
    <option value="end">Finishing at the end of the outgoing clip</option>
    <option value="start">From the start of the incoming clip</option>
  </select>
</div>
<div class="muted small">
  {p.placeAt === "end" ? "Ends at or just before the outgoing clip's end." : "Starts on the incoming clip's first beat."}
</div>
<div class="field">
  <label for="rs-cut">Hard cut falls</label>
  <select id="rs-cut" value={p.cutAt} onchange={(e) => recipePrefs.set({ cutAt: e.currentTarget.value as CutAt })}>
    <option value="end">At the end (incoming held back until the last beat)</option>
    <option value="start">At the start (incoming plays from the first beat)</option>
  </select>
</div>
<div class="field">
  <label for="rs-lo-len">Loop out: loop length</label>
  <select id="rs-lo-len" value={String(p.loopOutBeats)} onchange={(e) => recipePrefs.set({ loopOutBeats: Number(e.currentTarget.value) })}>
    {#each LOOP_BEATS as n (n)}<option value={String(n)}>{loopLengthLabel(n)}</option>{/each}
  </select>
</div>
<div class="field">
  <label for="rs-lo-at">Loop out: starts</label>
  <select id="rs-lo-at" value={p.loopOutStart} onchange={(e) => recipePrefs.set({ loopOutStart: e.currentTarget.value as LoopOutStart })}>
    <option value="start">At the start of the overlap</option>
    <option value="middle">In the middle of the overlap</option>
  </select>
</div>
<div class="muted small">
  Loops {loopLengthLabel(p.loopOutBeats)} of the outgoing track from {p.loopOutStart === "start" ? "the start" : "the middle"} of the overlap.
</div>
<div class="field">
  <label for="rs-lc-len">Loop cut: first loop</label>
  <select id="rs-lc-len" value={String(p.loopCutBeats)} onchange={(e) => recipePrefs.set({ loopCutBeats: Number(e.currentTarget.value) })}>
    {#each LOOP_BEATS as n (n)}<option value={String(n)}>{loopLengthLabel(n)}</option>{/each}
  </select>
</div>
<div class="field">
  <label for="rs-lc-min">Loop cut: shortest loop</label>
  <select id="rs-lc-min" value={String(p.loopCutShortest)} onchange={(e) => recipePrefs.set({ loopCutShortest: Number(e.currentTarget.value) })}>
    {#each LOOP_SHORTEST as n (n)}<option value={String(n)}>{loopLengthLabel(n)}</option>{/each}
  </select>
</div>
<div class="field">
  <label for="rs-lc-rep">Loop cut: times each length</label>
  <input
    id="rs-lc-rep"
    type="number"
    min={LOOP_REPEATS.min}
    max={LOOP_REPEATS.max}
    step="1"
    value={p.loopCutRepeats}
    onchange={(e) => {
      recipePrefs.set({ loopCutRepeats: e.currentTarget.valueAsNumber });
      e.currentTarget.value = String(recipePrefs.value.loopCutRepeats);
    }}
  />
</div>
<div class="field">
  <label for="rs-lc-rise">Loop cut: rising filter</label>
  <select id="rs-lc-rise" value={p.loopCutRising ? "on" : "off"} onchange={(e) => recipePrefs.set({ loopCutRising: e.currentTarget.value === "on" })}>
    <option value="off">Off</option>
    <option value="on">On: a high-pass rises over the loops</option>
  </select>
</div>
<div class="muted small">
  Loop cut plays {cutSteps.map(loopLengthLabel).join(", then ")}, {p.loopCutRepeats === 1 ? "once" : `${p.loopCutRepeats} times`} each. Longest are dropped if they do not fit.
</div>
<div class="field">
  <label for="rs-way">Tools in the way</label>
  <select id="rs-way" value={p.onConflict} onchange={(e) => recipePrefs.set({ onConflict: e.currentTarget.value as ConflictPolicy })}>
    <option value="ask">Ask, naming each one</option>
    <option value="skip">Keep mine, skip the new one</option>
    <option value="replace">Replace mine (Undo restores)</option>
  </select>
</div>
<div class="field">
  <label for="rs-sec">Sections</label>
  <select id="rs-sec" value={p.sectionEdge} onchange={(e) => recipePrefs.set({ sectionEdge: e.currentTarget.value as SectionEdge })}>
    <option value="off">Ignore sections</option>
    <option value="start">Start on a boundary</option>
    <option value="end">End on a boundary</option>
    <option value="both">Start and end on boundaries</option>
  </select>
</div>
{#if p.sectionEdge !== "off"}
  <div class="field">
    <label for="rs-reach">Boundary within (bars)</label>
    <input
      id="rs-reach"
      type="number"
      min={SECTION_REACH.min}
      max={SECTION_REACH.max}
      step="1"
      value={p.sectionReachBars}
      onchange={(e) => {
        recipePrefs.set({ sectionReachBars: e.currentTarget.valueAsNumber });
        e.currentTarget.value = String(recipePrefs.value.sectionReachBars);
      }}
    />
  </div>
  <div class="muted small">
    The transition {sectionEdgeWords(p.sectionEdge)} within {p.sectionReachBars} bar{p.sectionReachBars === 1 ? "" : "s"}. Needs detected sections.
  </div>
{/if}
<div class="field">
  <label for="rs-move">When a clip moves</label>
  <select id="rs-move" value={p.onMove} onchange={(e) => recipePrefs.set({ onMove: e.currentTarget.value as OnMove })}>
    <option value="follow">Crossfade and its tools follow the overlap</option>
    <option value="stay">They stay where they are</option>
  </select>
</div>
<div class="muted small">
  {p.onMove === "follow"
    ? "Moving a clip re-fits its crossfade and the tools the transition placed. Hand-placed crossfades stay put."
    : "Crossfades stay put when a clip moves. Fit to overlap saves the cut length."}
</div>
<div class="muted small">Applies to every Auto button and menu entry.</div>

<style>
  .small {
    font-size: 12px;
    margin: 4px 0;
  }
  .muted {
    color: var(--muted);
  }
</style>
