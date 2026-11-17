<script lang="ts">
  // The tempo-matching settings: when matching a track to the mix tempo is applied silently and
  // what happens to a track that would need a big stretch.
  import { onMount } from "svelte";
  import { mixLiveAutoMemory } from "$lib/api/mixLive";
  import { tempoPrefs } from "./tempoPrefs.svelte";

  const p = $derived(tempoPrefs.value);
  /** What "decide by the computer" gives here, in MB (null until the app answers). */
  let autoMb = $state<number | null>(null);
  onMount(() => {
    mixLiveAutoMemory().then(
      (mb) => (autoMb = mb),
      () => {},
    );
  });
</script>

<div class="field">
  <label for="ts-warn">Warn above</label>
  <span class="inline">
    <input id="ts-warn" type="number" min="0" max="100" step="1" value={p.warnPct} onchange={(e) => tempoPrefs.set({ warnPct: e.currentTarget.valueAsNumber })} />
    <span class="muted">% stretch</span>
  </span>
</div>
<div class="muted small">
  A match over this limit is held back and the clip gets a stretch badge.
</div>
<div class="field">
  <span class="lbl">New tracks</span>
  <label class="check"><input type="checkbox" checked={p.matchOnAdd} onchange={(e) => tempoPrefs.set({ matchOnAdd: e.currentTarget.checked })} /> match to the mix tempo when added</label>
</div>
<div class="field">
  <label for="ts-over">Over the limit</label>
  <select id="ts-over" value={p.overOnAdd} onchange={(e) => tempoPrefs.set({ overOnAdd: e.currentTarget.value as "keep" | "match" })}>
    <option value="keep">Keep its own tempo</option>
    <option value="match">Match anyway (badge shows the stretch)</option>
  </select>
</div>
<div class="field">
  <label for="ts-mem">Playback memory</label>
  <span class="inline">
    <input id="ts-mem" type="number" min="256" max="65536" step="256" disabled={p.decodeAuto} value={p.decodeAuto ? (autoMb ?? "") : p.decodeMb} onchange={(e) => tempoPrefs.set({ decodeMb: e.currentTarget.valueAsNumber })} />
    <span class="muted">MB</span>
    <label class="check"><input type="checkbox" checked={p.decodeAuto} onchange={(e) => tempoPrefs.set({ decodeAuto: e.currentTarget.checked })} /> decide by the computer</label>
  </span>
</div>
<div class="muted small">
  Decoded audio kept for live play (about 140 MB per 6-minute track). Applies the next time a mix is opened.
</div>

<style>
  .small {
    font-size: 12px;
    margin: 4px 0;
  }
  .lbl {
    color: var(--muted);
    font-size: 12px;
  }
  .inline {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .check {
    display: flex;
    gap: 6px;
    align-items: center;
    font-size: 13px;
  }
  .field {
    display: grid;
    grid-template-columns: 110px 1fr;
    gap: 6px;
    align-items: center;
    margin: 6px 0;
  }
  .field > label {
    color: var(--muted);
    font-size: 12px;
  }
</style>
