<script lang="ts">
  import { filters } from "$lib/state/filters.svelte";
  import { jobs } from "$lib/state/jobs.svelte";
  import { pickerOptions, sameRoot } from "$lib/state/place";
  import { session } from "$lib/state/session.svelte";
  import Button from "$lib/ui/Button.svelte";
  import RMark from "$lib/ui/RMark.svelte";

  // "This computer" first, then each detected Pacemaker; a folder opened by hand is added so the
  // picker always names what is open.
  const options = $derived(
    pickerOptions(session.computer?.path ?? null, session.computer?.exists ?? false, session.volumes, session.library),
  );
  const current = $derived(options.find((o) => session.library && sameRoot(o.value, session.library.root))?.value ?? "");

  function pick(value: string) {
    const c = session.computer;
    // choosing "This computer (new)" makes the library first
    if (c && sameRoot(value, c.path) && !c.exists) void session.startComputer();
    else void session.load(value);
  }
</script>

<header>
  <div class="brand"><RMark size={20} /><b>Pacemaker <span>Revival</span></b></div>
  <div class="source">
    {#if options.length > 0}
      <select aria-label="Library" value={current} onchange={(e) => pick((e.currentTarget as HTMLSelectElement).value)}>
        <option value="" disabled>Choose a library…</option>
        {#each options as o (o.value)}
          <option value={o.value}>{o.label}</option>
        {/each}
      </select>
    {/if}
    <Button onclick={() => session.refreshVolumes()} title="Look for a connected Pacemaker">Rescan</Button>
    <Button onclick={() => session.openFolder()}>Open folder…</Button>
  </div>
  {#if session.library}
    <Button
      variant="primary"
      onclick={() => jobs.pickFiles()}
      disabled={!jobs.canWrite}
      title={jobs.canWrite
        ? session.isComputer
          ? "Analyse audio files and copy them into the library on this computer"
          : "Analyse audio files and copy them to the Pacemaker"
        : "Unavailable while a job runs or the library is not device-compatible"}
    >Add tracks…</Button>
    <input
      class="search"
      placeholder="Search, or bpm:120-128 key:~8A tag:peak"
      title="Words match the title, artist, album, genre, key, comments, label, remixer, composer, year and tags. Fields, ranges, -not and OR work too: see the ? in the Tracks toolbar."
      bind:value={filters.query}
    />
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
    width: 300px;
  }
</style>
