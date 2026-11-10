<script lang="ts">
  // The state of a view that is not built yet. Says so plainly; never shows invented data.
  let { title, description, planned = [] }: { title: string; description: string; planned?: string[] } = $props();
</script>

<div class="placeholder">
  <h2>{title}</h2>
  <p>{description}</p>
  <p class="tag">Not built yet</p>
  {#if planned.length > 0}
    <ul>
      {#each planned as p (p)}<li>{p}</li>{/each}
    </ul>
  {/if}
</div>

<style>
  .placeholder {
    max-width: 520px;
    margin: 48px auto;
    padding: 0 20px;
    color: var(--muted);
  }
  h2 {
    margin: 0 0 6px;
    font-size: 18px;
    color: var(--text);
    font-weight: 600;
  }
  .tag {
    display: inline-block;
    padding: 0 8px;
    border-radius: 8px;
    font-size: 11px;
    background: var(--accent-soft);
    color: var(--accent);
  }
  ul {
    padding-left: 18px;
  }
</style>
