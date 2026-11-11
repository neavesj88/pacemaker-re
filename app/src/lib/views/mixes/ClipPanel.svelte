<script lang="ts">
  // Inspector for one clip: track facts, deck, tempo, key, gain, position, and the beat-match /
  // auto-transition actions.
  import type { Clip } from "$lib/api/mixes";
  import * as E from "$lib/state/mixedit";
  import { mixes } from "$lib/state/mixes.svelte";
  import { autoTransition, beatmatch, clipTitle, duplicateSelection, neighbours } from "./actions";
  import { barBeat, clock, fmtSemis, keyToEnv, SR } from "./layout";
  import { mv, type EnvLaneId } from "./view.svelte";

  let { clip, ondelete }: { clip: Clip; ondelete: () => void } = $props();

  const doc = $derived(mixes.doc!);
  const nb = $derived(neighbours(doc, clip.id));
  const eff = $derived(E.effectiveBpm(clip, clip.trimStart));
  const stretch = $derived(clip.bpm > 0 ? (eff / clip.bpm - 1) * 100 : 0);
  const mixBpm = $derived(doc.tempoBpm ?? eff);
  const pos = $derived(barBeat(clip.visibleStart, mixBpm));
  const keySemis = $derived(clip.key.length === 1 ? clip.key[0].cents / 100 : null);
  const flatGain = $derived(clip.gain.length > 0 && clip.gain.every((n) => n.value === clip.gain[0].value));
  const gain = $derived(clip.gain[0]?.value ?? 0);
  const keyEnv = $derived(keyToEnv(clip.key));

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
  function showLane(l: EnvLaneId) {
    mv.lanes[l] = !mv.lanes[l];
  }
  const semis = Array.from({ length: 25 }, (_, i) => i - 12);
</script>

<h3>{clipTitle(clip)}</h3>
<div class="muted">{clip.track?.artist || clip.fuzzyId.artist || "Unknown artist"}</div>

{#if clip.track && (!clip.track.available || clip.trackId < 0)}
  <div class="warn">{clip.track.problem ?? "This track is unavailable."} It stays in the mix but cannot be played, and the mix cannot be saved until it is removed or restored.</div>
{:else if clip.track && !clip.track.grid}
  <div class="warn">No beat grid ({clip.track.gridProblem ?? "not analysed"}). Snapping, beat-match and auto transition need one: analyse the track first.</div>
{/if}

<div class="field">
  <span class="lbl">Channel</span>
  <span class="seg">
    <button class:on={clip.channel === 0} onclick={() => mixes.setChannel(clip.id, 0)}>1 (A)</button>
    <button class:on={clip.channel === 1} onclick={() => mixes.setChannel(clip.id, 1)}>2 (B)</button>
  </span>
</div>
<div class="field"><span class="lbl">Track BPM</span><span class="val">{clip.bpm.toFixed(2)}</span></div>
<div class="field">
  <label for="cp-bpm">Plays at</label>
  <input
    id="cp-bpm"
    type="number"
    min="40"
    max="250"
    step="0.01"
    value={Number(eff.toFixed(2))}
    disabled={!(clip.bpm > 0)}
    onchange={(e) => {
      const v = e.currentTarget.valueAsNumber;
      if (v > 0) mixes.edit("Change tempo", (d) => E.setClipBpm(d, clip.id, v));
      else e.currentTarget.value = String(Number(eff.toFixed(2)));
    }}
  />
</div>
<div class="muted small">
  {#if clip.tempo.length > 1}tempo curve ({clip.tempo.length} nodes){:else}{stretch >= 0 ? "+" : "−"}{Math.abs(stretch).toFixed(1)} % {clip.pitchMode ? "(pitch follows)" : "time-stretched"}{/if}
</div>
<div class="field">
  <span class="lbl">Tempo mode</span>
  <select
    value={clip.pitchMode ? "pitch" : "stretch"}
    onchange={(e) => setPitchMode(e.currentTarget.value === "pitch")}
  >
    <option value="stretch">Time-stretch</option>
    <option value="pitch">Pitch (vinyl)</option>
  </select>
</div>
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
  <button disabled={!nb.prev} onclick={() => void beatmatch(clip.id)} title={nb.prev ? "Match tempo and beats to the previous clip" : "There is no earlier clip"}>Beat-match</button>
  <button disabled={!nb.next} onclick={() => nb.next && void autoTransition(clip.id, nb.next.id)} title="Crossfade into the next clip, with a bass swap">Auto transition</button>
</div>
<div class="buttons">
  <button onclick={duplicateSelection}>Duplicate</button>
  <button onclick={() => mixes.copy()}>Copy</button>
  <button class="danger" onclick={ondelete}>Delete…</button>
</div>

<h4>Envelope lanes</h4>
<div class="buttons">
  <button class:on={mv.lanes.tempo} onclick={() => showLane("tempo")}>Tempo</button>
  <button class:on={mv.lanes.key} onclick={() => showLane("key")}>Key</button>
  <button class:on={mv.lanes.gain} onclick={() => showLane("gain")}>Gain</button>
</div>
<p class="note">Beat-match moves this clip so its beats sit on the previous clip's; it changes the clip's tempo and start. Edit gain and key in their lanes: click to add a node, drag to move, right-click to remove.</p>

<style>
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
    gap: 6px;
    min-width: 0;
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
