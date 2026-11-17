<script lang="ts">
  import { appearance } from "$lib/state/appearance.svelte";
  import { DONE_SECS_CHOICES, ERROR_SECS_CHOICES, GRID_MIN_CHOICES, PLAYHEAD_FPS_CHOICES, SCALE_DEFAULT, SCALE_MAX, SCALE_MIN, SCALE_STEP, WAVE_COLOURS, WAVE_THEMES, WAVE_THEME_NAMES, normaliseWaveTheme } from "$lib/state/appearanceLogic";
  import { windowRememberGet, windowRememberSet } from "$lib/api/library";
  import { toast } from "$lib/state/toast.svelte";
  import Button from "$lib/ui/Button.svelte";

  // The size of the whole window (text, tables, the player), for a big monitor or a small laptop. It
  // changes at once and is remembered on this computer. Ctrl and + / Ctrl and - / Ctrl and 0 do the
  // same from the keyboard anywhere in the app.
  // while the slider is dragged only the number moves; the window changes when it is let go (zooming
  // under the pointer would move the slider away from it)
  let dragging = $state<number | null>(null);
  const shown = $derived(dragging ?? appearance.scale);

  // whether the window opens at the size it was closed at (kept by the app, so it is read from there)
  let remember = $state(true);
  void windowRememberGet().then((v) => (remember = v)).catch(() => {});
  async function setRemember(on: boolean) {
    const before = remember;
    remember = on;
    try {
      remember = await windowRememberSet(on);
    } catch (e) {
      remember = before;
      toast.error("Couldn't change that.", e);
    }
  }

  const gridText = (g: number) => (g === 0 ? "No beat grid" : `Bar lines down to ${g} px per bar${g === 2 ? " (most)" : ""}`);
  const secsText = (s: number) => (s === 0 ? "until I close them" : `${s} seconds`);
</script>

<div class="box">
  <div class="row">
    <label for="ui-scale" class="k">Interface size</label>
    <input
      id="ui-scale"
      type="range"
      min={SCALE_MIN}
      max={SCALE_MAX}
      step={SCALE_STEP}
      value={shown}
      oninput={(e) => (dragging = Number(e.currentTarget.value))}
      onchange={(e) => {
        appearance.setScale(Number(e.currentTarget.value));
        dragging = null;
      }}
      aria-valuetext="{shown} per cent"
    />
    <span class="num pct">{shown} %</span>
    <Button small disabled={appearance.scale === SCALE_DEFAULT} onclick={() => appearance.reset()}>Reset to {SCALE_DEFAULT} %</Button>
  </div>
  <p class="muted note">
    {SCALE_MIN} % to {SCALE_MAX} %. Ctrl and + larger, Ctrl and - smaller, Ctrl and 0 back to {SCALE_DEFAULT} %.
  </p>
  {#if appearance.zoomProblem}
    <p class="warn" role="alert">The window could not be resized: {appearance.zoomProblem}</p>
  {/if}

  <div class="row">
    <label class="k" for="win-size">Window size</label>
    <label class="tick"><input id="win-size" type="checkbox" checked={remember} onchange={(e) => void setRemember(e.currentTarget.checked)} /> Open at the size it was closed at</label>
  </div>
  <div class="row">
    <label for="err-secs" class="k">Error messages stay</label>
    <select id="err-secs" value={appearance.errorSecs} onchange={(e) => appearance.setErrorSecs(Number(e.currentTarget.value))}>
      {#each ERROR_SECS_CHOICES as s (s)}<option value={s}>{secsText(s)}</option>{/each}
    </select>
  </div>
  <div class="row">
    <label for="done-secs" class="k">"Finished" notes stay</label>
    <select id="done-secs" value={appearance.doneSecs} onchange={(e) => appearance.setDoneSecs(Number(e.currentTarget.value))}>
      {#each DONE_SECS_CHOICES as s (s)}<option value={s}>{secsText(s)}</option>{/each}
    </select>
  </div>
  <p class="muted note">Notes with failures stay until closed.</p>
  <div class="row">
    <label for="grid-min" class="k">Beat grid on the player</label>
    <select id="grid-min" value={appearance.gridMinBarPx} onchange={(e) => appearance.setGridMinBarPx(Number(e.currentTarget.value))}>
      {#each GRID_MIN_CHOICES as g (g)}<option value={g}>{gridText(g)}</option>{/each}
    </select>
  </div>
  <p class="muted note">Bar lines are hidden when a bar is narrower than this.</p>
  <div class="row">
    <label for="playhead-fps" class="k">Playhead smoothness</label>
    <select id="playhead-fps" value={appearance.playheadFps} onchange={(e) => appearance.setPlayheadFps(Number(e.currentTarget.value))}>
      {#each PLAYHEAD_FPS_CHOICES as f (f)}<option value={f}>{f === 0 ? "Every refresh of the screen" : f === 60 ? "Up to 60 a second" : `Up to ${f} a second`}</option>{/each}
    </select>
  </div>
  <p class="muted note">Fewer redraws use less processor time. Sound is not affected.</p>
  <div class="row" role="radiogroup" aria-label="Waveform colours">
    <span class="k">Waveform colours</span>
    {#each WAVE_THEMES as t (t)}
      <label class="choice">
        <input type="radio" name="wave-colours" value={t} checked={appearance.waveTheme === t} onchange={() => appearance.setWaveTheme(normaliseWaveTheme(t))} />
        {WAVE_THEME_NAMES[t]}
        <span class="swatch" aria-hidden="true">
          <i style:background={WAVE_COLOURS[t].low}></i><i style:background={WAVE_COLOURS[t].mid}></i><i style:background={WAVE_COLOURS[t].high}></i>
        </span>
      </label>
    {/each}
  </div>
  <p class="muted note">Mix clips keep their own colours.</p>
</div>

<style>
  .box {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 10px;
    flex-wrap: wrap;
  }
  .k {
    color: var(--muted);
    min-width: 130px;
  }
  .choice {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }
  .swatch {
    display: inline-flex;
    gap: 2px;
  }
  .swatch i {
    width: 14px;
    height: 14px;
    border-radius: 3px;
    border: 1px solid var(--line);
  }
  .pct {
    min-width: 48px;
  }
  .note {
    margin: 0;
  }
  .warn {
    margin: 0;
    color: var(--bad-text);
  }
  input[type="range"] {
    width: 220px;
  }
</style>
