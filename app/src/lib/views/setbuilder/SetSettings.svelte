<script lang="ts">
  import { recipePrefs } from "$lib/views/mixes/recipePrefs.svelte";
  import { transitionWords } from "./createMix";
  import EnergySourceSelect from "../metadata/EnergySourceSelect.svelte";
  import { setPrefs } from "./prefs.svelte";
  import { setBuilder } from "./setbuilder.svelte";
  import { KEY_MOVES, type KeyMoveId, type NumberPref, type SetPrefs } from "./solver";

  // Every choice the builder makes is here, with its default. Changing one re-scores the strip at
  // once; the order only changes when the set is solved again.
  let { hasEnergy }: { hasEnergy: boolean } = $props();

  let p = $derived(setPrefs.value);

  function patch(change: Partial<SetPrefs>) {
    setPrefs.set(change);
    setBuilder.settingsChanged();
  }

  /** A typed number: clamped into the setting's range, and the box shows what is now in force. */
  function typed(e: Event, key: NumberPref, nullable = false) {
    setPrefs.commitNumber(e.currentTarget as HTMLInputElement, key, nullable);
    setBuilder.settingsChanged();
  }

  function toggleMove(id: KeyMoveId, on: boolean) {
    patch({ allowedMoves: on ? [...p.allowedMoves, id] : p.allowedMoves.filter((x) => x !== id) });
  }
</script>

<aside class="side" aria-label="Set builder settings">
  <h3>Settings</h3>

  <fieldset>
    <legend>Key moves that count as good</legend>
    {#each KEY_MOVES as m (m.id)}
      <label class="check" title={m.hint}>
        <input type="checkbox" checked={p.allowedMoves.includes(m.id)} onchange={(e) => toggleMove(m.id, e.currentTarget.checked)} />
        <span>{m.label} <span class="muted">{m.hint}</span></span>
      </label>
    {/each}
    <p class="note muted">Keys are read as Camelot codes. A pair no ticked move covers is a clash. A track with no key is treated as neutral.</p>
  </fieldset>

  <fieldset>
    <legend>Tempo</legend>
    <label class="row">
      <span>Largest jump between neighbours (%)</span>
      <input type="number" min="0" max="50" step="0.5" value={p.maxBpmJumpPct} onchange={(e) => typed(e, "maxBpmJumpPct")} />
    </label>
    <label class="check">
      <input type="checkbox" checked={p.halfDouble} onchange={(e) => patch({ halfDouble: e.currentTarget.checked })} />
      <span>Half and double time count as matching <span class="muted">70 BPM after 140</span></span>
    </label>
    <label class="row">
      <span>Key {Math.round(p.keyWeight * 100)} % · tempo {Math.round((1 - p.keyWeight) * 100)} %</span>
      <input type="range" min="0" max="1" step="0.05" value={p.keyWeight} oninput={(e) => patch({ keyWeight: Number(e.currentTarget.value) })} aria-label="How much key counts against tempo" />
    </label>
  </fieldset>

  <fieldset>
    <legend>Energy curve</legend>
    <div class="row"><EnergySourceSelect compact /></div>
    <label class="row">
      <span>Shape</span>
      <select value={p.curve} onchange={(e) => patch({ curve: e.currentTarget.value as SetPrefs["curve"] })}>
        <option value="off">None</option>
        <option value="arc">Warm-up, peak, cool-down</option>
      </select>
    </label>
    {#if p.curve === "arc"}
      <label class="row">
        <span>Peak at {Math.round(p.peakAt * 100)} % of the set</span>
        <input type="range" min="0.3" max="0.9" step="0.05" value={p.peakAt} oninput={(e) => patch({ peakAt: Number(e.currentTarget.value) })} aria-label="Where the peak falls" />
      </label>
      <label class="row">
        <span>How strictly: {Math.round(p.energyWeight * 100)} %</span>
        <input type="range" min="0" max="1" step="0.05" value={p.energyWeight} oninput={(e) => patch({ energyWeight: Number(e.currentTarget.value) })} aria-label="How strictly the curve is followed" />
      </label>
    {/if}
    {#if !hasEnergy}
      <p class="note muted">None of these tracks has an energy under the Energy source setting (1 to 10), so the curve has nothing to follow and changes nothing. Rate tracks (Tracks, Tags, colour, energy), run Energy (auto) on them, or change the source above.</p>
    {/if}
  </fieldset>

  <fieldset>
    <legend>Length</legend>
    <label class="row">
      <span>Longest a track plays (minutes)</span>
      <input type="number" min="0.25" max="120" step="0.25" placeholder="whole track" value={p.maxTrackMins ?? ""} onchange={(e) => typed(e, "maxTrackMins", true)} />
    </label>
    <label class="row">
      <span>Aim for a set of (minutes)</span>
      <input type="number" min="1" max="1440" step="1" placeholder="use every track" value={p.targetMins ?? ""} onchange={(e) => typed(e, "targetMins", true)} />
    </label>
    <p class="note muted">With a target, the tracks that fit the set least are left out until the total is as close to it as it can get. It is the sum of the play times; the transitions overlap tracks, so a mix made from the set comes out a little shorter.</p>
  </fieldset>

  <fieldset>
    <legend>Behaviour</legend>
    <label class="check"><input type="checkbox" checked={p.solveOnLoad} onchange={(e) => patch({ solveOnLoad: e.currentTarget.checked })} /><span>Order the set as soon as tracks are loaded</span></label>
    <label class="check" title="Needs sections: Tracks, Sections panel. Only a tie-breaker between hops that are otherwise alike; tracks without sections are not penalised."><input type="checkbox" checked={p.preferSections} onchange={(e) => patch({ preferSections: e.currentTarget.checked })} /><span>Prefer leaving a track on its outro and arriving on an intro, where sections are detected</span></label>
    <label class="check"><input type="checkbox" checked={p.lockOnDrag} onchange={(e) => patch({ lockOnDrag: e.currentTarget.checked })} /><span>Lock a track where I drop it</span></label>
  </fieldset>

  <fieldset>
    <legend>When creating a mix</legend>
    <label class="check"><input type="checkbox" checked={p.beatmatch} onchange={(e) => patch({ beatmatch: e.currentTarget.checked })} /><span>Beat-match each track to the one before it</span></label>
    <label class="row">
      <span>Tempo the mix runs at (BPM)</span>
      <input type="number" min="40" max="250" step="0.1" placeholder="middle tempo of the set" disabled={!p.beatmatch} value={p.mixTempo ?? ""} onchange={(e) => typed(e, "mixTempo", true)} />
    </label>
    <label class="row">
      <span>Do not beat-match a hop bigger than (%)</span>
      <input type="number" min="0" max="50" step="0.5" placeholder="the largest tempo jump ({p.maxBpmJumpPct} %)" disabled={!p.beatmatch} value={p.blendJumpPct ?? ""} onchange={(e) => typed(e, "blendJumpPct", true)} />
    </label>
    <p class="note muted">A bigger hop is not stretched: that track starts when the one before it ends.</p>
    <label class="row">
      <span>Most a blend may overlap (% of the shorter track)</span>
      <input type="number" min="5" max="50" step="1" value={p.overlapCapPct} onchange={(e) => typed(e, "overlapCapPct")} />
    </label>
    <label class="row">
      <span>A tool overlaps one an earlier transition placed</span>
      <select value={p.onToolConflict} onchange={(e) => patch({ onToolConflict: e.currentTarget.value as SetPrefs["onToolConflict"] })}>
        <option value="skip">Leave the new tool out</option>
        <option value="replace">Replace the earlier tool</option>
      </select>
    </label>
    <p class="note muted">Transitions use the Mixes view's auto-transition recipe and length: {transitionWords(recipePrefs.value)}. Its “when a tool is in the way” setting does not apply here (a new mix holds none of your own tools, only ones this build places); the choice above does.</p>
  </fieldset>

  <button class="reset" onclick={() => (setPrefs.reset(), setBuilder.settingsChanged())}>Reset these settings</button>
</aside>

<style>
  .side {
    width: 280px;
    flex: none;
    box-sizing: border-box;
    padding: 12px;
    border-left: 1px solid var(--line);
    background: var(--panel);
    overflow: auto;
  }
  h3 {
    margin: 0 0 6px;
    font-size: 14px;
  }
  fieldset {
    border: none;
    border-top: 1px solid var(--line);
    margin: 10px 0 0;
    padding: 8px 0 0;
    display: grid;
    gap: 6px;
  }
  legend {
    padding: 0 6px 0 0;
    font-size: 11px;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--muted);
  }
  .row {
    display: grid;
    gap: 2px;
    font-size: 12px;
  }
  .row input[type="number"],
  .row select {
    width: 100%;
    box-sizing: border-box;
  }
  .check {
    display: flex;
    gap: 6px;
    align-items: flex-start;
    font-size: 12px;
  }
  .check input {
    margin-top: 2px;
  }
  .note {
    margin: 2px 0 0;
    font-size: 11px;
  }
  .reset {
    margin-top: 14px;
    background: none;
    border: 1px solid var(--line);
    color: var(--text);
    border-radius: 4px;
    padding: 4px 10px;
    font: inherit;
    cursor: pointer;
  }
  .reset:hover {
    border-color: var(--line-hi);
  }
</style>
