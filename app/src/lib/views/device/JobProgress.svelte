<script lang="ts">
  import { device } from "./device.svelte";

  // Progress of the running device job of one kind, if that is the one running.
  let { kind }: { kind: string } = $props();
  let job = $derived(device.job?.kind === kind ? device.job : null);
  let pct = $derived(job && job.total > 0 ? Math.round((job.done / job.total) * 100) : 0);
</script>

{#if job}
  <div class="prog" role="status">
    <div class="bar"><i style="width:{pct}%"></i></div>
    <div class="line muted">
      <span class="name">{job.label || "Starting…"}</span>
      <span class="num">{job.detail ?? (job.total > 0 ? `${job.done} of ${job.total}` : "")}</span>
    </div>
  </div>
{/if}

<style>
  .prog {
    margin-top: 10px;
  }
  .bar {
    height: 8px;
    border-radius: 4px;
    background: var(--row-alt);
    overflow: hidden;
  }
  .bar i {
    display: block;
    height: 100%;
    background: var(--accent-ramp);
    transition: width 0.2s;
  }
  .line {
    display: flex;
    justify-content: space-between;
    gap: 10px;
    margin-top: 4px;
    font-size: 12px;
  }
  .name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
