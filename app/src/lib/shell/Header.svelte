<script lang="ts">
  import { volumeName } from "$lib/format";
  import { filters } from "$lib/state/filters.svelte";
  import { jobs } from "$lib/state/jobs.svelte";
  import { session } from "$lib/state/session.svelte";
  import Button from "$lib/ui/Button.svelte";
  import RMark from "$lib/ui/RMark.svelte";
</script>

<header>
  <div class="brand"><RMark size={20} /><b>Pacemaker <span>Revival</span></b></div>
  <div class="source">
    {#if session.volumes.length > 0}
      <select onchange={(e) => session.load((e.currentTarget as HTMLSelectElement).value)}>
        <option value="" disabled selected={!session.library}>Choose a device…</option>
        {#each session.volumes as v}
          <option value={v} selected={session.library?.root === v}>{volumeName(v)}</option>
        {/each}
      </select>
    {/if}
    <Button onclick={() => session.refreshVolumes()} title="Look for connected devices">Rescan</Button>
    <Button onclick={() => session.openFolder()}>Open folder…</Button>
  </div>
  {#if session.library}
    <Button
      variant="primary"
      onclick={() => jobs.pickFiles()}
      disabled={!jobs.canWrite}
      title={jobs.canWrite
        ? "Analyse audio files and copy them to the device"
        : "Unavailable while a job runs or the library is not device-compatible"}
    >Add tracks…</Button>
    <input class="search" placeholder="Search" bind:value={filters.query} />
  {/if}
</header>

<style>
  header {
    display: flex;
    align-items: center;
    gap: 16px;
    padding: 8px 14px;
    background: var(--deep);
    border-bottom: 1px solid var(--line);
  }
  .brand {
    display: flex;
    align-items: center;
    gap: 8px;
    font-weight: 700;
    letter-spacing: 0.02em;
  }
  .brand span {
    color: var(--accent);
    font-weight: 400;
  }
  .source {
    display: flex;
    gap: 6px;
  }
  .search {
    margin-left: auto;
    width: 240px;
  }
</style>
