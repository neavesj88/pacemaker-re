<script lang="ts">
  import { jobs } from "$lib/state/jobs.svelte";
  import { session } from "$lib/state/session.svelte";
</script>

{#if jobs.dragging}
  <div class="dropzone">
    {session.library
      ? `Drop audio files to add them to ${session.library.root}`
      : "Open a Pacemaker library first, then drop audio files here"}
  </div>
{/if}

<style>
  .dropzone {
    position: fixed;
    inset: 0;
    z-index: 20;
    display: flex;
    align-items: center;
    justify-content: center;
    background: rgba(5, 6, 5, 0.88);
    border: 3px dashed var(--accent);
    color: var(--accent);
    font-size: 20px;
    pointer-events: none;
  }
</style>
