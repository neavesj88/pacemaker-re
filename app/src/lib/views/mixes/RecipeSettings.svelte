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
  {def.label} will be {lengthLabel(effective)}{p.bars != null && nearestLength(p.recipe, p.bars) !== p.bars ? ` (it offers ${def.lengths.map(lengthLabel).join(", ")}; the nearest is used)` : ""}. Never longer than the audio both clips share, and in whole bars.
</div>
<div class="field">
  <label for="rs-place">Crossfade sits</label>
  <select id="rs-place" value={p.placeAt} onchange={(e) => recipePrefs.set({ placeAt: e.currentTarget.value as CrossfadePlace })}>
    <option value="end">Finishing at the end of the outgoing clip</option>
    <option value="start">From the start of the incoming clip</option>
  </select>
</div>
<div class="muted small">
  {p.placeAt === "end" ? "The crossfade finishes at or just before the end of the outgoing clip's audio (up to a bar early, because it starts on a bar line of the incoming clip when that has a beat grid)." : "The crossfade begins on the incoming clip's first beat and runs for the length above, as far as the clips overlap."}
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
  Loop out repeats {loopLengthLabel(p.loopOutBeats)} of the outgoing track on its beat grid, from {p.loopOutStart === "start" ? "the start" : "the middle"} of the overlap until the crossfade has finished with it. A loop that does not fit twice is made shorter.
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
  Loop cut plays {cutSteps.map(loopLengthLabel).join(", then ")}, {p.loopCutRepeats === 1 ? "once" : `${p.loopCutRepeats} times`} each, one straight after the other, so the last one ends where the incoming track drops in. Each loop starts on the outgoing track's beat grid. If they do not all fit in the overlap, the longest are left out.
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
    The transition {sectionEdgeWords(p.sectionEdge)} (of either clip) within {p.sectionReachBars} bar{p.sectionReachBars === 1 ? "" : "s"} of where it would otherwise {p.sectionEdge === "end" ? "end" : "start"}, so its length can change. Only for clips whose tracks have detected sections (Sections panel); the notes after building say what moved. Without sections it builds exactly as above.
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
    ? "Drag, nudge, trim, Beat-align or change the tempo of a clip and the crossfade on its overlap, with the EQ and other tools the transition placed, re-fit the new overlap. A crossfade that filled the overlap fills the new one; one that ended with the outgoing clip keeps its length and keeps ending there; one you placed by hand stays put. Tools you added yourself in the middle of a fade ride with their clip. Undo puts it all back in one step."
    : "A crossfade stays where it was made when a clip moves, and only the tools on the moved clip go with it. Where the fade no longer sits on the overlap it is cut to what both clips share when played; Fit to overlap saves that."}
</div>
<div class="muted small">These apply to every Auto button and menu entry in this editor. The result is always ordinary tools and a crossfade curve you can edit.</div>

<style>
  .small {
    font-size: 12px;
    margin: 4px 0;
  }
  .muted {
    color: var(--muted);
  }
</style>
