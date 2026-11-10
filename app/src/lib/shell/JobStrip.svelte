<script lang="ts">
  import { jobs } from "$lib/state/jobs.svelte";
  import Button from "$lib/ui/Button.svelte";

  // Progress of the running add / analysis job, and its result until dismissed.
</script>

{#if jobs.job}
  {@const job = jobs.job}
  <div class="strip">
    {#if job.running}
      <div class="line">
        <b>{job.runningLabel} {job.done} / {job.total}</b>
        <span class="cur">{job.current || "starting…"}</span>
        {#if job.bpm}<span class="bpm">{job.bpm.toFixed(2)} BPM</span>{/if}
      </div>
      <progress max={job.total} value={job.done}></progress>
    {:else if job.result}
      <div class="line">
        <b>{job.doneLabel}:</b>
        <span class="cur">{job.result.summary || "nothing to do"}</span>
        <Button small onclick={() => jobs.dismiss()}>Dismiss</Button>
      </div>
    {/if}
    {#if job.failures.length > 0}
      <ul class="fails">
        {#each job.failures as f}<li><b>{f.label}</b>: {f.error}</li>{/each}
      </ul>
    {/if}
  </div>
{/if}

<style>
  .strip {
    padding: 6px 14px 8px;
    background: var(--panel);
    border-bottom: 1px solid var(--line);
    font-size: 12px;
  }
  .line {
    display: flex;
    gap: 10px;
    align-items: center;
  }
  .cur {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--muted);
  }
  .bpm {
    color: var(--green);
    font-variant-numeric: tabular-nums;
  }
  progress {
    width: 100%;
    height: 6px;
    margin-top: 4px;
    accent-color: var(--accent);
  }
  .fails {
    margin: 6px 0 0;
    padding-left: 18px;
    color: var(--bad-text);
    max-height: 90px;
    overflow: auto;
  }
</style>
