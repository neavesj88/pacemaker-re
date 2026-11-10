<script lang="ts">
  // 0-5 star rating. Read-only unless onchange is given; clicking the current rating clears it.
  let { rating, onchange }: { rating: number; onchange?: (rating: number) => void } = $props();
</script>

<span class="stars" role={onchange ? "group" : "img"} aria-label="{rating} of 5 stars">
  {#each [1, 2, 3, 4, 5] as n (n)}
    {#if onchange}
      <button class:on={n <= rating} aria-label="{n} star{n === 1 ? '' : 's'}" onclick={() => onchange(n === rating ? 0 : n)}>★</button>
    {:else}
      <span class:on={n <= rating}>★</span>
    {/if}
  {/each}
</span>

<style>
  .stars {
    display: inline-flex;
    gap: 1px;
    color: var(--line-hi);
    letter-spacing: 0;
  }
  .on {
    color: var(--accent);
  }
  button {
    background: none;
    border: none;
    padding: 0;
    color: inherit;
    font: inherit;
    cursor: pointer;
  }
</style>
