<script lang="ts">
  import { session } from "$lib/state/session.svelte";
</script>

{#if session.error}
  <div class="banner bad">{session.error}</div>
{/if}

{#if session.library}
  {#if session.library.problems.length === 0}
    <div class="banner ok">{session.library.root} · music.db is device-compatible</div>
  {:else}
    <div class="banner bad">
      This library would not load on the device:
      <ul>
        {#each session.library.problems as p}<li>{p}</li>{/each}
      </ul>
    </div>
  {/if}
{/if}

<style>
  .banner {
    padding: 6px 14px;
    font-size: 12px;
    border-bottom: 1px solid var(--line);
  }
  .ok {
    color: var(--muted);
  }
  .ok::before {
    content: "●  ";
    color: var(--ok);
  }
  .bad {
    background: var(--bad-bg);
    color: var(--bad-text);
  }
  ul {
    margin: 4px 0 0;
  }
</style>
