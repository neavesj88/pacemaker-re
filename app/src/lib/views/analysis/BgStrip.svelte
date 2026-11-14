<script lang="ts">
  // The compact status of the background analysis, in the job strip: one line with a thin
  // progress bar and Pause / Resume, and, below it, a small note when some tracks could not be
  // analysed. It is not a modal and never takes focus; a job you start shows above it.
  import { bg } from "$lib/state/bganalysis.svelte";
  import Button from "$lib/ui/Button.svelte";
  import { failureNote, failureRows, pauseAction, statusLine, stripVisible } from "./bgLogic";

  const s = $derived(bg.status);
  const visible = $derived(stripVisible(s));
  const action = $derived(pauseAction(s));
  const note = $derived(failureNote(bg.failures, bg.dismissedAt));
  let open = $state(false);
  const rows = $derived(open ? failureRows(bg.failures) : []);
</script>

{#if visible}
  <div class="bg" data-testid="bg-status" data-state={s.state}>
    <div class="line">
      <span class="text" class:warn={s.state === "blocked" || s.state === "error"}>{statusLine(s)}</span>
      {#if s.state === "running" && s.label}<span class="cur" title={s.label}>{s.label}</span>{/if}
      {#if action}
        <Button small onclick={() => bg.setPaused(action === "pause")}>{action === "pause" ? "Pause" : "Resume"}</Button>
      {/if}
    </div>
    {#if s.state === "running" && s.total > 0}<progress max={s.total} value={s.done}></progress>{/if}
  </div>
{/if}

{#if note}
  <div class="bg note" data-testid="bg-failures">
    <div class="line">
      <span class="text">{note}</span>
      <Button small variant="link" onclick={() => (open = !open)}>{open ? "Hide the list" : "Show the list"}</Button>
      <Button small onclick={() => bg.retryFailures()}>Try again</Button>
      <Button small variant="ghost" onclick={() => bg.dismissNote()} aria-label="Hide this note">Hide</Button>
    </div>
    {#if open}
      <ul class="fails">
        {#each rows as r (r.track_id)}<li><b>{r.label}</b> ({r.kinds}): {r.reasons}</li>{/each}
      </ul>
    {/if}
  </div>
{/if}

<style>
  .bg {
    padding: 4px 14px 6px;
    background: var(--panel);
    border-bottom: 1px solid var(--line);
    font-size: 12px;
  }
  .line {
    display: flex;
    gap: 10px;
    align-items: center;
    min-height: 24px;
  }
  .text {
    flex: none;
    color: var(--text);
  }
  .text.warn {
    color: var(--bad-text);
  }
  .cur {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--muted);
  }
  .note .text {
    flex: 1;
    color: var(--muted);
  }
  progress {
    width: 100%;
    height: 4px;
    margin-top: 2px;
    accent-color: var(--accent);
  }
  .fails {
    margin: 4px 0 0;
    padding-left: 18px;
    color: var(--muted);
    max-height: 120px;
    overflow: auto;
  }
</style>
