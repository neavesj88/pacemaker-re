<script lang="ts">
  import { jobs } from "$lib/state/jobs.svelte";
  import Button from "$lib/ui/Button.svelte";
  import Modal from "$lib/ui/Modal.svelte";

  // The confirmation naming the destination, shown before anything is written to the device.
  // Generic: the wording comes from the JobRequest the owning area built.
</script>

{#if jobs.pending}
  {@const p = jobs.pending}
  <Modal title={p.title} onclose={() => jobs.cancel()}>
    <p class="muted">{p.description}</p>
    <ul class="names">
      {#each p.names.slice(0, 6) as n}<li>{n}</li>{/each}
      {#if p.names.length > 6}<li class="muted">…and {p.names.length - 6} more</li>{/if}
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
    max-height: 140px;
    overflow: hidden;
  }
</style>
