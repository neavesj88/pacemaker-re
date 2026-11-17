<script lang="ts">
  // Settings > Analysis > Background analysis: the master switch, one switch per kind, the
  // playback wait, the key confidence floor, Run now (the only way a Pacemaker is analysed) and
  // the list of tracks that could not be analysed.
  import type { BgOnBattery } from "$lib/api/bganalysis";
  import { bg } from "$lib/state/bganalysis.svelte";
  import { session } from "$lib/state/session.svelte";
  import Button from "$lib/ui/Button.svelte";
  import HelpLink from "../help/HelpLink.svelte";
  import { clampPct, failureRows, heldBackRows, noResultRows, pauseAction, statusLine } from "./bgLogic";

  const s = $derived(bg.settings);
  const lib = $derived(session.library);
  const pacemaker = $derived(lib?.kind === "pacemaker");
  const rows = $derived(failureRows(bg.failures));
  const noResult = $derived(noResultRows(bg.failures));
  const heldBack = $derived(heldBackRows(bg.failures));
  const action = $derived(pauseAction(bg.status));

  function saveFloor(e: Event) {
    const input = e.currentTarget as HTMLInputElement;
    const value = clampPct(input.valueAsNumber);
    input.value = String(value); // show what was kept
    void bg.save({ key_min_confidence_pct: value });
  }
</script>

<label class="opt">
  <input type="checkbox" checked={s.enabled} onchange={(e) => bg.save({ enabled: e.currentTarget.checked })} />
  <span>
    Analyse in the background <HelpLink id="background-analysis" />
    <small class="muted">A Pacemaker is only analysed with Run now.</small>
  </span>
</label>

<div class="kinds" class:off={!s.enabled}>
  <label class="opt">
    <input type="checkbox" checked={s.waveforms} disabled={!s.enabled} onchange={(e) => bg.save({ waveforms: e.currentTarget.checked })} />
    <span>Waveforms <small class="muted">Quickest, so first.</small></span>
  </label>
  <label class="opt">
    <input type="checkbox" checked={s.keys} disabled={!s.enabled} onchange={(e) => bg.save({ keys: e.currentTarget.checked })} />
    <span>Keys <small class="muted">Only for tracks with no key. Never replaces yours.</small></span>
  </label>
  <label class="opt">
    <input type="checkbox" checked={s.energy} disabled={!s.enabled} onchange={(e) => bg.save({ energy: e.currentTarget.checked })} />
    <span>Energy</span>
  </label>
  <label class="opt">
    <input type="checkbox" checked={s.sections} disabled={!s.enabled} onchange={(e) => bg.save({ sections: e.currentTarget.checked })} />
    <span>Sections <small class="muted">Needs the beat grid.</small></span>
  </label>
</div>

<label class="opt">
  <input type="checkbox" checked={s.pause_during_playback} onchange={(e) => bg.save({ pause_during_playback: e.currentTarget.checked })} />
  <span>
    Wait while a track or mix preview plays
    <small class="muted">Resumes when playback stops.</small>
  </span>
</label>

<label class="opt">
  <input type="checkbox" checked={s.low_priority} onchange={(e) => bg.save({ low_priority: e.currentTarget.checked })} />
  <span>
    Analyse at low priority
    <small class="muted">Slower, same results. On by default with four processors or fewer.</small>
  </span>
</label>

<div class="opt">
  <span>
    <label for="bg-battery">When the computer is on battery</label>
    <select id="bg-battery" value={s.on_battery} onchange={(e) => bg.save({ on_battery: e.currentTarget.value as BgOnBattery })}>
      <option value="keep">Keep analysing</option>
      <option value="slow">Slow down (low priority)</option>
      <option value="pause">Wait until it is plugged in</option>
    </select>
    <small class="muted">Also slows analysis you start. No effect without a battery.</small>
  </span>
</div>

<label class="opt">
  <input type="checkbox" checked={s.hold_during_sync_plan} onchange={(e) => bg.save({ hold_during_sync_plan: e.currentTarget.checked })} />
  <span>
    Wait while a Sync plan is on screen
    <small class="muted">A key write would outdate the plan. Resumes when applied or cleared.</small>
  </span>
</label>

<label class="opt">
  <input type="number" min="0" max="100" step="5" value={s.key_min_confidence_pct} onchange={saveFloor} aria-label="Confidence floor for background keys, per cent" />
  <span>
    Confidence floor for background keys, per cent
    <small class="muted">Background analysis only. 0 writes every result.</small>
  </span>
</label>

<div class="status" data-testid="bg-settings-status">
  <span>{statusLine(bg.status)}</span>
  {#if action}
    <Button small onclick={() => bg.setPaused(action === "pause")}>{action === "pause" ? "Pause" : "Resume"}</Button>
  {/if}
  <Button small disabled={!lib || !s.enabled} onclick={() => bg.runNow()}>
    {pacemaker ? "Run now on this Pacemaker" : "Check this library now"}
  </Button>
  {#if bg.checkedAt}
    <span class="muted small" data-testid="bg-checked-at">Checked at {new Date(bg.checkedAt).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", second: "2-digit" })}</span>
  {/if}
</div>
{#if pacemaker}
  <p class="muted small">Run now reads the audio from the device. Keys are written to its library.</p>
{/if}

<div class="fail">
  <b>Couldn't be analysed</b>
  {#if rows.length === 0}
    <p class="muted small">None. Failed files are not retried until they change.</p>
  {:else}
    <ul>
      {#each rows as r (r.track_id)}<li><b>{r.label}</b> ({r.kinds}): {r.reasons}</li>{/each}
    </ul>
    <Button small onclick={() => bg.retryFailures()}>Try these again</Button>
  {/if}
  {#if heldBack.length > 0}
    <details class="nokey" data-testid="bg-held-back">
      <summary>{heldBack.length} {heldBack.length === 1 ? "track has" : "tracks have"} a key held back by the confidence floor</summary>
      <p class="muted small">Under the floor, so not written. Lower the floor to write them.</p>
      <ul>
        {#each heldBack as r (r.track_id)}<li><b>{r.label}</b> ({r.kinds}): {r.reasons}</li>{/each}
      </ul>
    </details>
  {/if}
  {#if noResult.length > 0}
    <details class="nokey" data-testid="bg-no-result">
      <summary>{noResult.length} {noResult.length === 1 ? "track has" : "tracks have"} no key to write (nothing to try again)</summary>
      <p class="muted small">Read fine, but no key in the audio.</p>
      <ul>
        {#each noResult as r (r.track_id)}<li><b>{r.label}</b> ({r.kinds}): {r.reasons}</li>{/each}
      </ul>
    </details>
  {/if}
</div>

<style>
  .opt {
    display: flex;
    gap: 10px;
    align-items: flex-start;
    margin-bottom: 10px;
  }
  .opt input[type="number"] {
    width: 64px;
    flex: none;
  }
  .opt input[type="checkbox"] {
    flex: none;
    margin-top: 3px;
  }
  .kinds {
    margin-left: 22px;
  }
  .kinds.off {
    opacity: 0.55;
  }
  small {
    display: block;
    margin-top: 2px;
  }
  .status {
    display: flex;
    flex-wrap: wrap;
    gap: 10px;
    align-items: center;
    margin: 12px 0 6px;
  }
  .status span {
    flex: 1;
    min-width: 220px;
  }
  .small {
    font-size: 12px;
  }
  .fail {
    margin-top: 14px;
  }
  .nokey {
    margin-top: 10px;
  }
  .fail ul {
    margin: 6px 0 8px;
    padding-left: 18px;
    max-height: 160px;
    overflow: auto;
    font-size: 12px;
  }
</style>
