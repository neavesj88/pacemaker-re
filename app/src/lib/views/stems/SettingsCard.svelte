<script lang="ts">
  import { pickRuntimeLibrary } from "$lib/api/stems";
  import { stems } from "$lib/state/stems.svelte";
  import Button from "$lib/ui/Button.svelte";
  import { sameSettings, settingsProblem } from "./logic";

  // Every choice here is a setting with a default; nothing is fixed in the code. Changing one
  // does nothing until Save, and clears the plan under review so a preview never describes
  // settings other than the ones the run will use.
  const d = $derived(stems.draft);
  const saved = $derived(stems.status?.settings ?? null);
  const dirty = $derived(!!d && !!saved && !sameSettings(d, saved));
  const problem = $derived(d ? settingsProblem(d) : null);
  let saving = $state(false);

  async function save() {
    saving = true;
    await stems.saveSettings();
    saving = false;
  }

  async function chooseRuntime() {
    if (!d) return;
    const p = await pickRuntimeLibrary().catch(() => null);
    if (p) d.runtime_path = p;
  }
</script>

<section class="card">
  <header><h4>Settings</h4></header>
  {#if !d}
    <p class="muted">Reading the settings…</p>
  {:else}
    {#if stems.status?.settings_warning}<p class="warn" role="status">{stems.status.settings_warning}</p>{/if}

    <div class="grid">
      <label for="st-device">Run on</label>
      <select id="st-device" bind:value={d.device}>
        <option value="auto">Automatic (graphics card if the engine can, else CPU)</option>
        <option value="cpu">CPU only</option>
        <option value="gpu">Graphics card only (fails if the engine cannot)</option>
      </select>

      <label for="st-threads">CPU threads</label>
      <div class="inline">
        <input id="st-threads" type="number" min="0" max="64" step="1" bind:value={d.threads} />
        <span class="muted">0 = automatic</span>
      </div>

      <label for="st-format">File format</label>
      <select id="st-format" bind:value={d.format}>
        <option value="mp3">MP3 (the format the Pacemaker is known to play)</option>
        <option value="ogg">Ogg Vorbis</option>
        <option value="wav">WAV, 16-bit (large)</option>
      </select>

      {#if d.format === "mp3"}
        <label for="st-kbps">MP3 bitrate</label>
        <select id="st-kbps" bind:value={d.mp3_kbps}>
          {#each [128, 160, 192, 224, 256, 320] as k (k)}<option value={k}>{k} kbps</option>{/each}
        </select>
        <span></span>
        <label class="check"><input type="checkbox" bind:checked={d.mp3_align} /> Keep the MP3 in time with the original (drops the encoder's first 25 ms so the beat grid still fits)</label>
      {:else if d.format === "ogg"}
        <label for="st-q">Ogg quality</label>
        <div class="inline">
          <input id="st-q" type="number" min="-0.1" max="1" step="0.1" bind:value={d.ogg_quality} />
          <span class="muted">-0.1 smallest to 1.0 best</span>
        </div>
      {/if}

      <label for="st-inst">Instrumental is</label>
      <select id="st-inst" bind:value={d.instrumental}>
        <option value="sum">The model's drums + bass + other</option>
        <option value="minus">The original minus the vocals (keeps the original's character; vocal leftovers stay in)</option>
      </select>

      <label for="st-overlap">Overlap between parts</label>
      <div class="inline">
        <input id="st-overlap" type="number" min="10" max="90" step="5" bind:value={d.overlap_percent} />
        <span class="muted">% — more overlap is smoother and slower</span>
      </div>

      <label for="st-max">Longest track</label>
      <div class="inline">
        <input id="st-max" type="number" min="0" max="600" step="5" bind:value={d.max_minutes} />
        <span class="muted">minutes, 0 = no limit (a track is held in memory while it is separated)</span>
      </div>

      <label for="st-vs">Acapella suffix</label>
      <input id="st-vs" bind:value={d.vocals_suffix} maxlength="40" />
      <label for="st-is">Instrumental suffix</label>
      <input id="st-is" bind:value={d.instrumental_suffix} maxlength="40" />

      <label for="st-ex">Files that already exist</label>
      <select id="st-ex" bind:value={d.existing}>
        <option value="skip">Keep them and only make what is missing</option>
        <option value="replace">Replace them (every file is listed and confirmed first)</option>
      </select>

      <label for="st-rt">Engine library</label>
      <div class="inline">
        <input id="st-rt" bind:value={d.runtime_path} placeholder="empty = the downloaded engine" />
        <Button small onclick={chooseRuntime}>Choose…</Button>
      </div>
    </div>
    <p class="muted note">
      The downloaded engine runs on the CPU. To use a graphics card, choose an onnxruntime library that was built with DirectML or CUDA. Which graphics cards a given library can use was not tested here.
    </p>

    {#if problem}<p class="warn" role="alert">{problem}</p>{/if}
    <div class="actions">
      <Button small disabled={!dirty || saving} onclick={() => stems.resetDraft()}>Undo changes</Button>
      <Button small variant="primary" disabled={!dirty || !!problem || saving || stems.busy} onclick={save}>{saving ? "Saving…" : "Save settings"}</Button>
    </div>
  {/if}
</section>

<style>
  .card {
    background: var(--panel);
    border: 1px solid var(--line);
    border-radius: 8px;
    padding: 12px;
    min-width: 0;
  }
  header {
    margin-bottom: 8px;
  }
  h4 {
    margin: 0;
    font-size: 13px;
  }
  .grid {
    display: grid;
    grid-template-columns: 150px minmax(0, 1fr);
    gap: 6px 10px;
    align-items: center;
  }
  .grid label {
    font-size: 12px;
  }
  .grid select,
  .grid input:not([type="checkbox"]) {
    width: 100%;
    min-width: 0;
    box-sizing: border-box;
  }
  .grid input[type="number"] {
    width: 90px;
  }
  .inline {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }
  .inline .muted {
    font-size: 12px;
  }
  .inline input:not([type="number"]) {
    flex: 1;
  }
  .check {
    display: flex;
    gap: 6px;
    align-items: flex-start;
  }
  .note {
    font-size: 12px;
    margin: 8px 0 0;
  }
  .warn {
    margin: 8px 0 0;
    padding: 6px 10px;
    border-radius: 4px;
    background: var(--bad-bg);
    color: var(--bad-text);
    font-size: 12px;
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 10px;
  }
</style>
