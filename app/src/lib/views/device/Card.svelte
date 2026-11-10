<script lang="ts">
  import type { Snippet } from "svelte";

  // A titled panel of the device view (the mockup's .card).
  let { title, aside, children }: { title: string; aside?: Snippet; children?: Snippet } = $props();
</script>

<section class="card">
  <header>
    <h4>{title}</h4>
    {@render aside?.()}
  </header>
  {@render children?.()}
</section>

<style>
  .card {
    background: var(--panel);
    border: 1px solid var(--line);
    border-radius: 8px;
    padding: 12px;
    min-width: 0;
  }
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    margin-bottom: 8px;
  }
  h4 {
    margin: 0;
  }
</style>
