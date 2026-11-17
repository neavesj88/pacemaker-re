<script lang="ts">
  // Inspector for one crossfade: which clips it joins, its fader curve, type, beat-match.
  import { untrack } from "svelte";
  import type { EnvNode, TransitionDoc } from "$lib/api/mixes";
  import * as E from "$lib/state/mixedit";
  import { mixes } from "$lib/state/mixes.svelte";
  import EnvelopeSvg from "./EnvelopeSvg.svelte";
  import RecipeSettings from "./RecipeSettings.svelte";
  import { autoTransition, beatAlign, beatmatch, clipTitle } from "./actions";
  import { CURVE_PRESETS, CURVE_SCALE, SR, barBeat, clock, curveName, curveToEnv, envToCurve } from "./layout";
  import { recipePrefs } from "./recipePrefs.svelte";
  import { RECIPES, lengthLabel, nearestLength, recipeDef, requestedBars, type RecipeId } from "$lib/state/transitionRecipes";

  let { t, ondelete }: { t: TransitionDoc; ondelete: () => void } = $props();

  const doc = $derived(mixes.doc!);
  const from = $derived(t.between ? E.clipOf(doc, t.between.from) : undefined);
  const to = $derived(t.between ? E.clipOf(doc, t.between.to) : undefined);
  const bpm = $derived(doc.tempoBpm ?? (to ? E.effectiveBpm(to, to.trimStart) : 120));
  /** Where it is heard (inside the audio both clips share); the stored range when they do not overlap. */
  const heard = $derived(E.effectiveFade(doc, t));
  const at = $derived(heard ?? { start: t.start, length: t.length, clamped: false });
  const bars = $derived((at.length / SR) / ((60 / bpm) * 4));
  const name = $derived(curveName(t.curve));
  let boxW = $state(250);
  let selNode = $state(-1);

  function patch(label: string, fn: (x: TransitionDoc) => TransitionDoc) {
    mixes.edit(label, (d) => E.updateTransition(d, t.id, fn));
  }
  function setCurve(nodes: EnvNode[]) {
    mixes.setCurve(t.id, envToCurve(nodes), { live: true });
  }

  // The recipe to rebuild this crossfade with; starts from the one it was built with in this session, else the defaults.
  // (the panel is keyed by the transition, so reading its id once is right)
  const made = untrack(() => recipePrefs.built.get(t.id));
  let recipe = $state<RecipeId>(made?.recipe ?? recipePrefs.value.recipe);
  let recipeBars = $state<number>(made ? nearestLength(made.recipe, made.bars) : requestedBars(recipePrefs.value.recipe, recipePrefs.value));
  const def = $derived(recipeDef(recipe));

  function pickRecipe(id: RecipeId) {
    recipe = id;
    const want = recipePrefs.value.bars;
    recipeBars = want == null ? recipeDef(id).bars : nearestLength(id, want);
  }
  function rebuild() {
    if (from && to) void autoTransition(from.id, to.id, { recipe, bars: recipeBars, replacing: t.id });
  }
</script>

<h3>Crossfade</h3>
<div class="muted">
  {#if from && to}“{clipTitle(from)}” into “{clipTitle(to)}”{:else}between two clips (not matched to clips by the layout){/if}
</div>

<div class="field"><span class="lbl">Starts</span><span class="val">bar {barBeat(at.start, bpm).bar} · {clock(at.start / SR)}</span></div>
<div class="field"><span class="lbl">Length</span><span class="val">{bars.toFixed(bars % 1 === 0 ? 0 : 1)} bars · {clock(at.length / SR)}</span></div>
{#if !heard}
  <div class="muted small">The clips do not overlap here, so this crossfade does nothing. Move a clip or delete it.</div>
{:else if at.clamped}
  <div class="muted small">
    Reaches past the shared audio (stored: {(t.length / SR).toFixed(1)} s). Playback and export cut it to the overlap.
    <button onclick={() => mixes.edit("Fit crossfade to overlap", (d) => E.fitTransition(d, t.id))} title="Store the cut length">Fit to overlap</button>
  </div>
{/if}
<div class="field">
  <span class="lbl">Active</span>
  <label class="check"><input type="checkbox" checked={!t.bypassed} onchange={(e) => patch(e.currentTarget.checked ? "Enable crossfade" : "Mute crossfade", (x) => ({ ...x, bypassed: !e.currentTarget.checked }))} /> {t.bypassed ? "muted" : "on"}</label>
</div>

<h4>Curve · {name}</h4>
<div class="presets">
  {#each CURVE_PRESETS as pr (pr.name)}
    <button class:on={name === pr.name} onclick={() => mixes.setCurve(t.id, pr.curve)}>{pr.name}</button>
  {/each}
</div>
<div class="curvebox" bind:clientWidth={boxW}>
  <EnvelopeSvg
    nodes={curveToEnv(t.curve)}
    d0={0}
    d1={CURVE_SCALE}
    min={0}
    max={1}
    width={Math.max(120, boxW - 2)}
    height={110}
    colour="var(--accent)"
    guides={[0, 1]}
    label="Crossfade curve"
    lockEnds
    minNodes={2}
    snapValue={(v) => Math.round(v * 100) / 100}
    onchange={(n) => setCurve(n)}
    ondone={() => mixes.endGesture()}
    bind:selected={selNode}
  />
</div>
<div class="muted small">Low = earlier clip, high = later clip.</div>

<h4>Type</h4>
<div class="field">
  <label for="tp-type">Fade after</label>
  <select id="tp-type" value={String(t.crossfadeType)} onchange={(e) => patch("Crossfade type", (x) => ({ ...x, crossfadeType: Number(e.currentTarget.value) }))}>
    <option value="1">1/2 of the overlap</option>
    <option value="3">3/4 of the overlap</option>
    {#if t.crossfadeType !== 1 && t.crossfadeType !== 3}<option value={String(t.crossfadeType)}>type {t.crossfadeType}</option>{/if}
  </select>
</div>
<div class="field">
  <span class="lbl">Beat-match</span>
  <label class="check"><input type="checkbox" checked={t.beatMatch} onchange={(e) => patch("Beat-match flag", (x) => ({ ...x, beatMatch: e.currentTarget.checked }))} /> flag the crossfade</label>
</div>

<h4>Recipe</h4>
<div class="field">
  <label for="tp-recipe">Recipe</label>
  <select id="tp-recipe" value={recipe} onchange={(e) => pickRecipe(e.currentTarget.value as RecipeId)}>
    {#each RECIPES as r (r.id)}<option value={r.id}>{r.label}</option>{/each}
  </select>
</div>
<div class="field">
  <label for="tp-bars">Length</label>
  <select id="tp-bars" value={String(recipeBars)} onchange={(e) => (recipeBars = Number(e.currentTarget.value))}>
    {#each def.lengths as n (n)}<option value={String(n)}>{lengthLabel(n)}</option>{/each}
  </select>
</div>
<div class="muted small">{def.summary}</div>
<ul class="makes">
  {#each def.makes as m (m)}<li>{m}</li>{/each}
</ul>
<div class="buttons">
  <button disabled={!from || !to} onclick={rebuild} title="Replace this crossfade with the recipe">Rebuild as {def.label.toLowerCase()}</button>
</div>
<details class="defaults">
  <summary>Auto transition settings</summary>
  <RecipeSettings />
</details>

<div class="buttons">
  <button disabled={!to} onclick={() => to && void beatmatch(to.id)} title="Match the later clip's tempo and beats to the clip before it">Beat-match clips</button>
  <button disabled={!from || !to} onclick={() => beatAlign()} title="Move the later clip, tempo unchanged, to line its beat grid up with the earlier clip's. Clips without a grid stay.">Beat-align</button>
  <button class="danger" onclick={ondelete}>Delete…</button>
</div>

<style>
  .small {
    font-size: 12px;
    margin-top: 4px;
  }
  .lbl {
    color: var(--muted);
    font-size: 12px;
  }
  .check {
    display: flex;
    gap: 6px;
    align-items: center;
    color: var(--text);
    font-size: 13px;
  }
  .makes {
    margin: 4px 0 0;
    padding-left: 18px;
    font-size: 12px;
    color: var(--muted);
  }
  .defaults {
    margin-top: 10px;
    font-size: 12px;
  }
  .defaults summary {
    cursor: pointer;
    color: var(--muted);
  }
  .presets {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    margin-bottom: 6px;
  }
  .presets button {
    padding: 1px 8px;
    font-size: 12px;
  }
  .presets button.on {
    background: var(--accent-soft);
    color: var(--accent);
    border-color: var(--accent);
  }
</style>
