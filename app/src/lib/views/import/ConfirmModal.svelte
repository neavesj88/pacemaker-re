<script lang="ts">
  import { jobs } from "$lib/state/jobs.svelte";
  import Button from "$lib/ui/Button.svelte";
  import Modal from "$lib/ui/Modal.svelte";

  // The confirmation naming the destination, shown before anything is written to the device.
  // Generic: the wording comes from the JobRequest the owning area built. Every item is listed
  // (the list scrolls) so the user sees exactly what will be written.
  const MAX_LISTED = 500;
</script>

{#if jobs.pending}
  {@const p = jobs.pending}
  <Modal title={p.title} onclose={() => jobs.cancel()}>
    <p class="muted">{p.description}</p>
    <ul class="names">
      {#each p.names.slice(0, MAX_LISTED) as n}<li>{n}</li>{/each}
      {#if p.names.length > MAX_LISTED}<li class="muted">…and {p.names.length - MAX_LISTED} more</li>{/if}
    </ul>
    {#snippet actions()}
      <Button data-autofocus onclick={() => jobs.cancel()}>Cancel</Button>
      <Button variant="primary" onclick={() => jobs.confirm()}>{p.confirmLabel}</Button>
    {/snippet}
  </Modal>
{/if}

<style>
  .names {
    margin: 8px 0 0;
    padding-left: 18px;
    max-height: 260px;
    overflow: auto;
  }
</style>
