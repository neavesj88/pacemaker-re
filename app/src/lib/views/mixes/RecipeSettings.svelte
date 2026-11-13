<script lang="ts">
  // The auto-transition settings: which recipe the Auto buttons build, how long, where a hard cut
  // falls, and what happens to tools of yours that are in the way. Remembered between sessions.
  import { RECIPES, SECTION_REACH, lengthLabel, nearestLength, recipeDef, requestedBars, sectionEdgeWords, type RecipeId } from "$lib/state/transitionRecipes";
  import type { CutAt, ConflictPolicy, SectionEdge } from "$lib/state/transitionRecipes";
  import { recipePrefs } from "./recipePrefs.svelte";

  const p = $derived(recipePrefs.value);
  const def = $derived(recipeDef(p.recipe));
  const effective = $derived(requestedBars(p.recipe, p));
</script>

<div class="field">
  <label for="rs-recipe">Recipe</label>
  <select id="rs-recipe" value={p.recipe} onchange={(e) => recipePrefs.set({ recipe: e.currentTarget.value as RecipeId })}>
    {#each RECIPES as r (r.id)}<option value={r.id}>{r.label}</option>{/each}
  </select>
</div>
<div class="field">
  <label for="rs-bars">Length</label>
  <select id="rs-bars" value={p.bars == null ? "auto" : String(p.bars)} onchange={(e) => recipePrefs.set({ bars: e.currentTarget.value === "auto" ? null : Number(e.currentTarget.value) })}>
    <option value="auto">Each recipe's own</option>
    {#each [0.25, 0.5, 1, 2, 4, 8, 16, 32, 64] as n (n)}<option value={String(n)}>{lengthLabel(n)}</option>{/each}
  </select>
</div>
<div class="muted small">
  {def.label} will be {lengthLabel(effective)}{p.bars != null && nearestLength(p.recipe, p.bars) !== p.bars ? ` (it offers ${def.lengths.map(lengthLabel).join(", ")}; the nearest is used)` : ""}. Shorter if the clips share less audio.
</div>
<div class="field">
  <label for="rs-cut">Hard cut falls</label>
  <select id="rs-cut" value={p.cutAt} onchange={(e) => recipePrefs.set({ cutAt: e.currentTarget.value as CutAt })}>
    <option value="end">At the end (incoming held back until the last beat)</option>
    <option value="start">At the start (incoming plays from the first beat)</option>
  </select>
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
