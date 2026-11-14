<script lang="ts">
  import { filters } from "$lib/state/filters.svelte";
  import { jobs } from "$lib/state/jobs.svelte";
  import { nav } from "$lib/state/nav.svelte";
  import { missingComputerPath, pickerOptions, sameRoot } from "$lib/state/place";
  import { session } from "$lib/state/session.svelte";
  import Button from "$lib/ui/Button.svelte";
  import RMark from "$lib/ui/RMark.svelte";

  // "This computer" first, then each detected Pacemaker; a folder opened by hand is added so the
  // picker always names what is open.
  // A saved path that is not there (an unplugged drive) is not offered: choosing it would quietly make a
  // second, empty library. The Welcome screen explains it and asks first.
  const options = $derived(
    pickerOptions(
      missingComputerPath(session.computerOffer) ? null : (session.computerOffer?.path ?? null),
      session.computerOffer?.exists ?? false,
      session.volumes,
      session.attached,
    ),
  );
  const current = $derived(options.find((o) => session.attached && sameRoot(o.value, session.attached.root))?.value ?? "");

  async function pick(select: HTMLSelectElement) {
    const value = select.value;
    // keep showing the open library until the switch really happens (a question may stop it)
    select.value = current;
    const c = session.computer;
    // choosing "This computer (new)" makes the library first
    if (c && sameRoot(value, c.path) && !c.exists) await session.startComputer();
    else await session.load(value);
    // the select is bound one way: when the read failed or the question was cancelled nothing
    // changed in `current`, so the box would keep showing the library that did not open
    select.value = current;
  }
</script>

<header>
  <div class="brand"><RMark size={20} /><b>Pacemaker <span>Revival</span></b></div>
  <div class="source">
    {#if options.length > 0}
      <select aria-label="Library" value={current} disabled={session.loading} onchange={(e) => void pick(e.currentTarget as HTMLSelectElement)}>
        <option value="" disabled>Choose a library…</option>
        {#each options as o (o.value)}
          <option value={o.value} title={o.title}>{o.label}</option>
        {/each}
      </select>
    {/if}
    <Button onclick={() => session.refreshVolumes()} title="Look for a connected Pacemaker">Rescan</Button>
    <Button disabled={session.loading} onclick={() => session.openFolder()}>Open folder…</Button>
  </div>
  {#if session.library && session.isComputer && session.volumes.length > 0}
    <Button
      onclick={() => nav.go("sync", { direction: "send", other: session.volumes[0] })}
      title="Copy tracks, mixes and cases from this computer to the Pacemaker. You see what would change first."
    >Send to Pacemaker</Button>
  {/if}
  {#if session.library}
    <Button
      variant="primary"
      onclick={() => jobs.pickFiles()}
      disabled={!jobs.canWrite}
      title={jobs.canWrite
        ? session.isComputer
          ? "Analyse audio files and copy them into the library on this computer"
          : "Analyse audio files and copy them to the Pacemaker"
        : "Unavailable while a job runs, the library is not device-compatible or the Pacemaker was ejected"}
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
  /* one line each, even at a narrow window: the search box gives way instead */
  header :global(button),
  .brand {
    white-space: nowrap;
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
    flex: 0 1 300px;
    min-width: 100px;
    width: 300px;
  }
</style>
