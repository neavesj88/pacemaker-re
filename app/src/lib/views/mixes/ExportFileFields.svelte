<script lang="ts">
  // Shared by the export dialog and the Settings view: the same state (`mixExport.prefs`, kept on
  // this computer), so a change in one place is the change in the other.
  import { mixExport as X } from "$lib/state/mixexport.svelte";
  import { AUDIO_FORMATS, FLAC_LEVELS, MP3_CBR_KBPS, MP3_EFFORT_DEFAULT, MP3_VBR_LEVELS, OGG_QUALITY_DEFAULT, OGG_QUALITY_RANGE } from "$lib/state/exportprefs";

  X.ensureLoaded();

  function changed() {
    X.save();
  }

  const fmt = $derived(AUDIO_FORMATS.find((f) => f.id === X.prefs.format));
</script>

<div class="row">
  <label for="xf-format">Format</label>
  <select id="xf-format" bind:value={X.prefs.format} onchange={changed}>
    {#each AUDIO_FORMATS as f (f.id)}<option value={f.id}>{f.label}</option>{/each}
  </select>
  <span class="muted small">{fmt?.hint ?? ""}</span>
</div>
{#if X.prefs.format === "ogg"}
  <div class="row">
    <label for="xf-ogg">Quality</label>
    <select id="xf-ogg" bind:value={X.prefs.oggQuality} onchange={changed}>
      {#each Array.from({ length: OGG_QUALITY_RANGE[1] - OGG_QUALITY_RANGE[0] + 1 }, (_, i) => OGG_QUALITY_RANGE[0] + i) as q (q)}
        <option value={q}>{(q / 10).toFixed(1)}{q === OGG_QUALITY_DEFAULT ? " (default)" : q === OGG_QUALITY_RANGE[0] ? " (smallest)" : q === OGG_QUALITY_RANGE[1] ? " (best)" : ""}</option>
      {/each}
    </select>
    <span class="muted small">higher is better and bigger</span>
  </div>
{:else if X.prefs.format === "mp3"}
  <div class="row">
    <label for="xf-mode">Bitrate</label>
    <select id="xf-mode" bind:value={X.prefs.mp3Mode} onchange={changed}>
      <option value="cbr">Constant (the same every second)</option>
      <option value="vbr">Variable (by quality)</option>
    </select>
    {#if X.prefs.mp3Mode === "cbr"}
      <select aria-label="Constant bitrate" bind:value={X.prefs.mp3Kbps} onchange={changed}>
        {#each MP3_CBR_KBPS as k (k)}<option value={k}>{k} kbit/s</option>{/each}
      </select>
    {:else}
      <select aria-label="Variable bitrate quality" bind:value={X.prefs.mp3Quality} onchange={changed}>
        {#each MP3_VBR_LEVELS as l (l.quality)}<option value={l.quality}>V{l.quality}: about {l.kbps} kbit/s{l.quality === 0 ? " (best)" : l.quality === 9 ? " (smallest)" : ""}</option>{/each}
      </select>
    {/if}
  </div>
  <div class="row">
    <label for="xf-stereo">Channels</label>
    <select id="xf-stereo" bind:value={X.prefs.mp3Stereo} onchange={changed}>
      <option value="joint">Joint stereo (smaller at the same quality)</option>
      <option value="stereo">Stereo (each channel coded on its own)</option>
    </select>
    <label for="xf-effort">Encoder effort</label>
    <select id="xf-effort" bind:value={X.prefs.mp3Effort} onchange={changed}>
      {#each Array.from({ length: 10 }, (_, i) => i) as e (e)}
        <option value={e}>{e}{e === MP3_EFFORT_DEFAULT ? " (default)" : e === 0 ? " (best, slowest)" : e === 9 ? " (fastest)" : ""}</option>
      {/each}
    </select>
  </div>
{:else if X.prefs.format === "flac"}
  <div class="row">
    <label for="xf-bits">Sample size</label>
    <select id="xf-bits" bind:value={X.prefs.flacBits} onchange={changed}>
      <option value={16}>16-bit (dithered, like a WAV)</option>
      <option value={24}>24-bit</option>
    </select>
    <label for="xf-level">Compression</label>
    <select id="xf-level" bind:value={X.prefs.flacLevel} onchange={changed}>
      {#each FLAC_LEVELS as l (l.id)}<option value={l.id}>{l.label}</option>{/each}
    </select>
  </div>
{/if}

<style>
  .row {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 8px;
    margin: 6px 0;
  }
  .row label {
    color: var(--muted);
    font-size: 12px;
  }
  .small {
    font-size: 12px;
  }
</style>
