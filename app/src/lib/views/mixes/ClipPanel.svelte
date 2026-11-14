<script lang="ts">
  // Inspector for one clip: track facts, tempo (fine steps, pitch per cent, sync to the mix or to
  // the clip before, ramps), key, gain, position, tools and the beat-match / auto-transition actions.
  import type { Clip } from "$lib/api/mixes";
  import * as E from "$lib/state/mixedit";
  import { mixes } from "$lib/state/mixes.svelte";
  import { exceeds, stretchText, type TempoChoice } from "$lib/state/tempoMatch";
  import ContextMenu, { type MenuItem } from "$lib/ui/ContextMenu.svelte";
  import { addProcessTo, autoTransition, beatmatch, clipTitle, duplicateSelection, neighbours, toolChoices } from "./actions";
  import BpmField from "./BpmField.svelte";
  import { barBeat, clock, fmtSemis, keyToEnv, SR } from "./layout";
  import { toolPrefs } from "./listPrefs.svelte";
  import RecipeSettings from "./RecipeSettings.svelte";
  import { recipePrefs } from "./recipePrefs.svelte";
  import { tempoPrefs } from "./tempoPrefs.svelte";
  import { lengthLabel, recipeDef, requestedBars } from "$lib/state/transitionRecipes";
  import { mv, type EnvOverlay } from "./view.svelte";

  let { clip, ondelete }: { clip: Clip; ondelete: () => void } = $props();

  const doc = $derived(mixes.doc!);
  const nb = $derived(neighbours(doc, clip.id));
  const eff = $derived(E.effectiveBpm(clip, clip.trimStart));
  const speed = $derived(clip.bpm > 0 ? eff / clip.bpm : 1);
  const pct = $derived((speed - 1) * 100);
  const mixBpm = $derived(doc.tempoBpm ?? eff);
  const pos = $derived(barBeat(clip.visibleStart, mixBpm));
  const keySemis = $derived(clip.key.length === 1 ? clip.key[0].cents / 100 : null);
  const flatGain = $derived(clip.gain.length > 0 && clip.gain.every((n) => n.value === clip.gain[0].value));
  const gain = $derived(clip.gain[0]?.value ?? 0);
  const keyEnv = $derived(keyToEnv(clip.key));
  const warnPct = $derived(tempoPrefs.value.warnPct);
  const tooMuch = $derived(exceeds(speed, warnPct));
  /** The tempo of the clip before this one where it ends: what "Sync to previous clip" matches. */
  const prevBpm = $derived(nb.prev ? E.effectiveBpm(nb.prev, nb.prev.length - nb.prev.trimEnd) : null);

  /** A match that was held back for stretching too much, with what it would do. */
  let held = $state<{ what: string; target: number; choice: TempoChoice } | null>(null);
  let menu = $state<{ x: number; y: number; items: MenuItem[] } | null>(null);

  function setKey(semis: number) {
    mixes.setEnvelope(clip.id, { kind: "key" }, [{ pos: 0, cents: Math.round(semis * 100), flags: clip.key[0]?.flags ?? 1 }]);
  }
  function setGain(db: number, live: boolean) {
    const last = Math.max(1, Math.round(clip.length) - 1);
    const nodes = flatGain && clip.gain.length > 0 ? clip.gain.map((n) => ({ ...n, value: db })) : [{ pos: 0, value: db }, { pos: last, value: db }];
    mixes.setEnvelope(clip.id, { kind: "gain" }, nodes, { live });
  }
  function setPitchMode(pitch: boolean) {
    mixes.edit("Tempo mode", (d) => E.updateClip(d, clip.id, (c) => ({ ...c, pitchMode: pitch })));
  }
  function showOverlay(o: EnvOverlay) {
    mv.overlay = mv.overlay === o ? "none" : o;
  }
  const semis = Array.from({ length: 25 }, (_, i) => i - 12);

  // ---- tempo ----------------------------------------------------------------------------------

  const setBpm = (v: number) => {
    held = null;
    mixes.edit("Change tempo", (d) => E.setClipBpm(d, clip.id, v), { group: `bpm:${clip.id}` });
  };
  const setPct = (v: number) => {
    held = null;
    mixes.edit("Change tempo", (d) => E.setClipPercent(d, clip.id, v), { group: `bpm:${clip.id}` });
  };
  const nudgePct = (d: number) => setPct(Math.round((pct + d) * 100) / 100);

  /** Matches the clip to `target` (or its double / half). A match that stretches more than the
   * setting allows is held back and shown with its choices instead of applied. */
  function sync(target: number | null, what: string) {
    if (!target) return;
    const r = mixes.syncClip(clip.id, target);
    held = r.applied || r.already || !r.choice ? null : { what, target, choice: r.choice };
  }
  function matchAnyway() {
    if (!held) return;
    mixes.syncClip(clip.id, held.target, true);
    held = null;
  }
  function ownTempo() {
    held = null;
    mixes.edit("Keep own tempo", (d) => E.setClipBpm(d, clip.id, clip.bpm));
  }

  const hasRamp = $derived(clip.tempo.length > 1);
  function addRampNode() {
    const local = Math.min(Math.max(clip.trimStart, mv.playhead - clip.start), clip.length - clip.trimEnd);
    mixes.edit("Add tempo node", (d) => E.setTempoNodeAt(d, clip.id, local, new E.TempoMap(clip.tempo).speedAt(local)));
    mv.overlay = "tempo";
  }
  function flattenTempo() {
    mixes.edit("Flatten tempo", (d) => E.setClipBpm(d, clip.id, eff));
  }

  function toolMenu(e: MouseEvent) {
    e.stopPropagation(); // the menu closes on any click that reaches the window
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const at = Math.min(Math.max(mv.playhead, clip.visibleStart), clip.visibleEnd - E.MIN_LENGTH);
    menu = { x: r.left, y: r.bottom + 2, items: toolChoices(toolPrefs.showTrans).map((t) => ({ label: t.label, onclick: () => addProcessTo(t.type, clip, at, t.vocal) })) };
  }
</script>

<h3>{clipTitle(clip)}</h3>
<div class="muted">{clip.track?.artist || clip.fuzzyId.artist || "Unknown artist"}</div>

{#if clip.track && (!clip.track.available || clip.trackId < 0)}
  <div class="warn">{clip.track.problem ?? "This track is unavailable."} It stays in the mix but cannot be played, and the mix cannot be saved until it is removed or restored.</div>
{:else if clip.track && !clip.track.grid}
  <div class="warn">No beat grid ({clip.track.gridProblem ?? "not analysed"}). Snapping, beat-match and auto transition need one: analyse the track first.</div>
{/if}

<h4>Tempo</h4>
<div class="field"><span class="lbl">Track BPM</span><span class="val">{clip.bpm.toFixed(2)}</span></div>
<div class="field">
  <label for="cp-bpm">Plays at</label>
  <BpmField id="cp-bpm" value={eff} onset={setBpm} label="Plays at" disabled={!(clip.bpm > 0)} />
</div>
<div class="field">
  <span class="lbl">Octave</span>
  <span class="inline">
    <button class="nudge" disabled={!(clip.bpm > 0)} title="Half time: play at half the current tempo (the other octave, when the automatic match chose wrongly)" onclick={() => setBpm(eff / 2)}>×½ half time</button>
    <button class="nudge" disabled={!(clip.bpm > 0)} title="Double time: play at twice the current tempo" onclick={() => setBpm(eff * 2)}>×2 double time</button>
  </span>
</div>
<div class="field">
  <label for="cp-pct">Pitch / speed</label>
  <span class="inline">
    <button class="nudge" title="0.1 % slower" onclick={() => nudgePct(-0.1)}>−.1</button>
    <button class="nudge" title="0.01 % slower" onclick={() => nudgePct(-0.01)}>−.01</button>
    <input
      id="cp-pct"
      type="number"
      step="0.01"
      min="-80"
      max="300"
      value={Number(pct.toFixed(2))}
      disabled={!(clip.bpm > 0)}
      onchange={(e) => {
        const v = e.currentTarget.valueAsNumber;
        if (Number.isFinite(v)) setPct(v);
        else e.currentTarget.value = String(Number(pct.toFixed(2)));
      }}
    />
    <span class="muted">%</span>
    <button class="nudge" title="0.01 % faster" onclick={() => nudgePct(0.01)}>+.01</button>
    <button class="nudge" title="0.1 % faster" onclick={() => nudgePct(0.1)}>+.1</button>
  </span>
</div>
<div class="muted small">
  {#if hasRamp}tempo ramp ({clip.tempo.length} nodes), {stretchText(pct)} where the clip starts{:else}{stretchText(pct)} {clip.pitchMode ? "(the pitch follows)" : "time-stretched, the pitch stays"}{/if}
</div>
{#if tooMuch}
  <div class="warn">
    This track plays {stretchText(pct)} of its own tempo ({clip.bpm.toFixed(1)} to {eff.toFixed(1)} BPM), more than the {warnPct} % limit in the settings.
    <div class="buttons"><button onclick={ownTempo}>Keep its own tempo</button></div>
  </div>
{/if}
<div class="buttons">
  <button disabled={!doc.tempoBpm} onclick={() => sync(doc.tempoBpm, "the mix tempo")} title="Play this track at the mix tempo, or double or half of it, whichever stretches it least">Sync to mix tempo</button>
  <button disabled={!prevBpm} onclick={() => sync(prevBpm, "the clip before")} title={nb.prev ? "Play this track at the tempo the clip before it ends on (or double or half of it)" : "There is no earlier clip"}>Sync to previous clip</button>
</div>
{#if held}
  <div class="warn">
    Matching to {held.what} ({held.target.toFixed(2)} BPM) would play this track at {held.choice.playsAt.toFixed(2)} BPM: {stretchText(held.choice.stretchPct)} stretch, more than the {warnPct} % limit
    ({held.choice.factor === 1 ? "same time" : held.choice.factor === 2 ? "double time" : "half time"}). Nothing was changed.
    <div class="buttons"><button onclick={matchAnyway}>Match anyway</button><button onclick={ownTempo}>Keep its own tempo</button></div>
  </div>
{/if}
<div class="field">
  <span class="lbl">Tempo mode</span>
  <select value={clip.pitchMode ? "pitch" : "stretch"} onchange={(e) => setPitchMode(e.currentTarget.value === "pitch")}>
    <option value="stretch">Time-stretch</option>
    <option value="pitch">Pitch (vinyl)</option>
  </select>
</div>
<div class="buttons">
  <button onclick={addRampNode} title="Add a tempo node at the playhead and show the tempo curve: drag the node up or down for a ramp (linear between nodes)">Add ramp node at playhead</button>
  {#if hasRamp}<button onclick={flattenTempo} title="Replace the tempo curve with one constant tempo (the one where the clip starts)">Flatten ramp</button>{/if}
</div>

<h4>Sound</h4>
<div class="field">
  <label for="cp-key">Key</label>
  <span class="inline">
    <span class="val">{clip.track?.key || "–"}</span>
    <select id="cp-key" value={keySemis === null ? "curve" : String(keySemis)} onchange={(e) => e.currentTarget.value !== "curve" && setKey(Number(e.currentTarget.value))}>
      {#if keySemis === null}<option value="curve">curve ({keyEnv.length} nodes)</option>{/if}
      {#each semis as s (s)}<option value={String(s)}>{fmtSemis(s)}</option>{/each}
    </select>
  </span>
</div>
<div class="field">
  <label for="cp-gain">Gain</label>
  {#if flatGain}
    <span class="inline">
      <input id="cp-gain" type="range" min="-12" max="6" step="0.1" value={gain} oninput={(e) => setGain(e.currentTarget.valueAsNumber, true)} onchange={() => mixes.endGesture()} />
      <span class="val">{gain.toFixed(1)} dB</span>
    </span>
  {:else}
    <span class="inline"
      ><span class="val">automated</span><button class="small-btn" onclick={() => setGain(clip.gain[0]?.value ?? 0, false)} title="Replace the gain curve with its first value">Flatten</button></span
    >
  {/if}
</div>

<h4>Position</h4>
<div class="field"><span class="lbl">Starts</span><span class="val">bar {pos.bar} · beat {pos.beat} · {clock(clip.visibleStartSec)}</span></div>
<div class="field"><span class="lbl">Audible</span><span class="val">{clock((clip.visibleEnd - clip.visibleStart) / SR)}</span></div>
{#if clip.trimStart > 0 || clip.trimEnd > 0}
  <div class="muted small">
    Trimmed: {clock(clip.trimStart / SR)} cue{clip.trimEnd > 0 ? `, ${clock(clip.trimEnd / SR)} cut from the end` : ""}. Drag the clip edges to change.
  </div>
{/if}

<h4>Actions</h4>
<div class="buttons">
  <button onclick={toolMenu} title="Add a tool, loop, reverse or ignore to this clip at the playhead">+ Tool ▾</button>
  <button disabled={!nb.prev} onclick={() => void beatmatch(clip.id)} title={nb.prev ? "Match tempo and beats to the previous clip (half or double time when that stretches this track less)" : "There is no earlier clip"}>Beat-match</button>
  <button disabled={!nb.next} onclick={() => nb.next && void autoTransition(clip.id, nb.next.id)} title="Transition into the next clip with the recipe set below">Auto transition</button>
</div>
<details class="defaults">
  <summary>{recipeDef(recipePrefs.value.recipe).label}, {lengthLabel(requestedBars(recipePrefs.value.recipe, recipePrefs.value))}</summary>
  <RecipeSettings />
</details>
<div class="buttons">
  <button onclick={duplicateSelection}>Duplicate</button>
  <button onclick={() => mixes.copy()}>Copy</button>
  <button class="danger" onclick={ondelete}>Delete…</button>
</div>

<h4>Curves over the bar</h4>
<div class="buttons">
  <button class:on={mv.overlay === "tempo"} onclick={() => showOverlay("tempo")}>Tempo</button>
  <button class:on={mv.overlay === "key"} onclick={() => showOverlay("key")}>Key</button>
  <button class:on={mv.overlay === "gain"} onclick={() => showOverlay("gain")}>Gain</button>
</div>
<p class="note">
  Beat-match moves this clip so its beats sit on the previous clip's; it changes the clip's tempo and start. The curves are drawn over the clips on the bar (one kind at a time): with a
  curve on, click on the selected clip to add a node, drag to move it, right-click to remove it; drag the clip by its name.
</p>

{#if menu}
  <ContextMenu x={menu.x} y={menu.y} items={menu.items} onclose={() => (menu = null)} />
{/if}

<style>
  .defaults {
    margin-top: 6px;
    font-size: 12px;
  }
  .defaults summary {
    cursor: pointer;
    color: var(--muted);
  }
  .small {
    font-size: 12px;
  }
  .lbl {
    color: var(--muted);
    font-size: 12px;
  }
  .val {
    white-space: nowrap;
  }
  .inline {
    display: flex;
    align-items: center;
    gap: 3px;
    min-width: 0;
  }
  .inline input[type="number"] {
    flex: 1 1 46px;
    min-width: 42px;
    text-align: center;
  }
  .nudge {
    flex: none;
    padding: 0 3px;
    font-size: 10px;
    white-space: nowrap;
  }
  .small-btn {
    padding: 1px 6px;
    font-size: 11px;
  }
  .buttons button.on {
    background: var(--accent-soft);
    color: var(--accent);
    border-color: var(--accent);
  }
</style>
