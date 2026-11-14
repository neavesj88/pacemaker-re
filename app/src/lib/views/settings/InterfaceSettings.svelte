<script lang="ts">
  import { appearance } from "$lib/state/appearance.svelte";
  import { ERROR_SECS_CHOICES, GRID_MIN_CHOICES, SCALE_DEFAULT, SCALE_MAX, SCALE_MIN, SCALE_STEP } from "$lib/state/appearanceLogic";
  import Button from "$lib/ui/Button.svelte";

  // The size of the whole window (text, tables, the player), for a big monitor or a small laptop. It
  // changes at once and is remembered on this computer. Ctrl and + / Ctrl and - / Ctrl and 0 do the
  // same from the keyboard anywhere in the app.
  // while the slider is dragged only the number moves; the window changes when it is let go (zooming
  // under the pointer would move the slider away from it)
  let dragging = $state<number | null>(null);
  const shown = $derived(dragging ?? appearance.scale);

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
    {SCALE_MIN} % to {SCALE_MAX} %. Ctrl and + make it bigger, Ctrl and - smaller, Ctrl and 0 back to {SCALE_DEFAULT} %. The window
    needs room for the size you pick: if things no longer fit, choose a smaller one.
  </p>
  {#if appearance.zoomProblem}
    <p class="warn" role="alert">The window could not be resized: {appearance.zoomProblem}</p>
  {/if}

  <div class="row">
    <label for="err-secs" class="k">Error messages stay</label>
    <select id="err-secs" value={appearance.errorSecs} onchange={(e) => appearance.setErrorSecs(Number(e.currentTarget.value))}>
      {#each ERROR_SECS_CHOICES as s (s)}<option value={s}>{secsText(s)}</option>{/each}
    </select>
  </div>
  <div class="row">
    <label for="grid-min" class="k">Beat grid on the player</label>
    <select id="grid-min" value={appearance.gridMinBarPx} onchange={(e) => appearance.setGridMinBarPx(Number(e.currentTarget.value))}>
      {#each GRID_MIN_CHOICES as g (g)}<option value={g}>{gridText(g)}</option>{/each}
    </select>
  </div>
  <p class="muted note">
    Bar lines on the player's waveform stop being drawn when a bar gets narrower than this, so a long track on a narrow window
    is not covered in lines. A lower number keeps the grid on longer tracks.
  </p>
  <p class="muted note">Other messages (saved, copied) disappear by themselves after a few seconds. Any message has a × to close it.</p>
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
