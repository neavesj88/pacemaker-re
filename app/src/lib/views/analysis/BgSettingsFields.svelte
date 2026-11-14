<script lang="ts">
  // Settings > Analysis > Background analysis: the master switch, one switch per kind, the
  // playback wait, the key confidence floor, Run now (the only way a Pacemaker is analysed) and
  // the list of tracks that could not be analysed.
  import { bg } from "$lib/state/bganalysis.svelte";
  import { session } from "$lib/state/session.svelte";
  import Button from "$lib/ui/Button.svelte";
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
    Analyse in the background
    <small class="muted">
      Makes the waveform, key, energy and sections of every track in the library on this computer while you do something
      else, in any view, and carries on after a restart. A Pacemaker is never read by itself: use Run now below.
    </small>
  </span>
</label>

<div class="kinds" class:off={!s.enabled}>
  <label class="opt">
    <input type="checkbox" checked={s.waveforms} disabled={!s.enabled} onchange={(e) => bg.save({ waveforms: e.currentTarget.checked })} />
    <span>Waveforms <small class="muted">First, because they are the quickest.</small></span>
  </label>
  <label class="opt">
    <input type="checkbox" checked={s.keys} disabled={!s.enabled} onchange={(e) => bg.save({ keys: e.currentTarget.checked })} />
    <span>Keys <small class="muted">Written to the library's key column, for tracks that have none. A key you set is never replaced.</small></span>
  </label>
  <label class="opt">
    <input type="checkbox" checked={s.energy} disabled={!s.enabled} onchange={(e) => bg.save({ energy: e.currentTarget.checked })} />
    <span>Energy <small class="muted">Kept by this app, not written into the library.</small></span>
  </label>
  <label class="opt">
    <input type="checkbox" checked={s.sections} disabled={!s.enabled} onchange={(e) => bg.save({ sections: e.currentTarget.checked })} />
    <span>Sections <small class="muted">Kept by this app, not written into the library. Needs the track's beat grid.</small></span>
  </label>
</div>

<label class="opt">
  <input type="checkbox" checked={s.pause_during_playback} onchange={(e) => bg.save({ pause_during_playback: e.currentTarget.checked })} />
  <span>
    Wait while a track or mix preview plays
    <small class="muted">Keeps the analysis from competing with playback for the processor. It carries on when playback stops.</small>
  </span>
</label>

<label class="opt">
  <input type="checkbox" checked={s.hold_during_sync_plan} onchange={(e) => bg.save({ hold_during_sync_plan: e.currentTarget.checked })} />
  <span>
    Wait while a Sync plan is on screen
    <small class="muted">A key written now would change a row the plan was made from, and Sync would refuse the plan as out of date. It carries on when the plan is applied or cleared.</small>
  </span>
</label>

<label class="opt">
  <input type="number" min="0" max="100" step="5" value={s.key_min_confidence_pct} onchange={saveFloor} aria-label="Confidence floor for background keys, per cent" />
  <span>
    Confidence floor for background keys, per cent
    <small class="muted">
      Applies to background analysis only; the manual Key detection section has its own floor. A key found below this is not written; the track is listed below as a
      key held back by the confidence floor instead, and is looked at again as soon as you change the floor. 0 writes every result.
    </small>
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
  <p class="muted small">
    Run now reads the audio of the tracks that lack a result from the device. Waveforms, energy and sections are kept on this computer; keys it finds are written to the
    Pacemaker's library. Nothing runs on the device until you press it.
  </p>
{/if}

<div class="fail">
  <b>Couldn't be analysed</b>
  {#if rows.length === 0}
    <p class="muted small">None. A file that fails once is listed here and not tried again until it changes or you ask.</p>
  {:else}
    <ul>
      {#each rows as r (r.track_id)}<li><b>{r.label}</b> ({r.kinds}): {r.reasons}</li>{/each}
    </ul>
    <Button small onclick={() => bg.retryFailures()}>Try these again</Button>
  {/if}
  {#if heldBack.length > 0}
    <details class="nokey" data-testid="bg-held-back">
      <summary>{heldBack.length} {heldBack.length === 1 ? "track has" : "tracks have"} a key held back by the confidence floor</summary>
      <p class="muted small">A key was found but it is under the floor set here, so it was not written. Lower the floor to write it.</p>
      <ul>
        {#each heldBack as r (r.track_id)}<li><b>{r.label}</b> ({r.kinds}): {r.reasons}</li>{/each}
      </ul>
    </details>
  {/if}
  {#if noResult.length > 0}
    <details class="nokey" data-testid="bg-no-result">
      <summary>{noResult.length} {noResult.length === 1 ? "track has" : "tracks have"} no key to write (nothing to try again)</summary>
      <p class="muted small">These were read fine. A drum-only, silent or atonal track has no key in it, so there is nothing to try again.</p>
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
