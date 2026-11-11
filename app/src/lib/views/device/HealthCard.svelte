<script lang="ts">
  import { jobs } from "$lib/state/jobs.svelte";
  import { nav } from "$lib/state/nav.svelte";
  import { selection } from "$lib/state/selection.svelte";
  import { session } from "$lib/state/session.svelte";
  import Button from "$lib/ui/Button.svelte";
  import Card from "./Card.svelte";
  import { device } from "./device.svelte";
  import { bytes, plural } from "./format";

  let h = $derived(device.health);
  let needs = $derived(session.needs.size);

  // "Show tracks" lands in the tracks view with the affected rows highlighted
  // the first few affected tracks by name, so they can be found without leaving this card
  let missingNames = $derived.by(() => {
    const byId = new Map((session.library?.tracks ?? []).map((t) => [t.track_id, t]));
    const names = (h?.missing_audio ?? []).map((m) => {
      const t = byId.get(m.track_id);
      return t?.title || m.location.split("/").pop() || m.location;
    });
    return names.length > 4 ? `${names.slice(0, 4).join(", ")} and ${names.length - 4} more` : names.join(", ");
  });

  function showMissing() {
    selection.trackIds = (h?.missing_audio ?? []).map((m) => m.track_id);
    nav.go("tracks");
  }
</script>

<Card title="Library health">
  {#snippet aside()}
    <Button
      small
      disabled={device.scanning || !!device.job || !session.library}
      onclick={() => session.library && device.load(session.library.root)}
    >
      {device.scanning ? "Scanning…" : "Scan again"}
    </Button>
  {/snippet}

  {#if !h}
    <p class="muted">{device.scanning ? "Checking every file on the drive…" : "Not checked yet."}</p>
  {:else}
    <ul>
      <li class={h.compatibility_problems.length ? "bad" : "ok"}>
        <span class="mark">{h.compatibility_problems.length ? "✗" : "✓"}</span>
        <div class="what">
          {#if h.compatibility_problems.length}
            <b>The database is not device-compatible.</b> The library stays read-only until this is fixed.
            <ul class="problems">
              {#each h.compatibility_problems as p (p)}<li>{p}</li>{/each}
            </ul>
          {:else}
            Database matches what the device requires.
          {/if}
        </div>
      </li>

      <li class={h.missing_audio.length ? "bad" : "ok"}>
        <span class="mark">{h.missing_audio.length ? "✗" : "✓"}</span>
        <div class="what">
          {#if h.missing_audio.length}
            {plural(h.missing_audio.length, "track")} listed in the library but without an audio file on the drive: {missingNames}.
          {:else}
            Every track has its audio file.
          {/if}
        </div>
        {#if h.missing_audio.length}
          <Button small onclick={showMissing}>Show tracks</Button>
          <Button small disabled={device.blocked} onclick={() => (device.fixOpen = true)}>Fix�</Button>
        {/if}
      </li>

      <li class={needs ? "bad" : "ok"}>
        <span class="mark">{needs ? "✗" : "✓"}</span>
        <div class="what">
          {#if needs}
            {plural(needs, "track")} without a usable beat grid (.str missing, empty or unreadable).
          {:else}
            Every track has a beat grid.
          {/if}
        </div>
        {#if needs}<Button small onclick={() => jobs.askReanalyse()}>Analyse…</Button>{/if}
      </li>

      <li class={!h.unreferenced_checked ? "warn" : h.unreferenced.length ? "bad" : "ok"}>
        <span class="mark">{!h.unreferenced_checked ? "?" : h.unreferenced.length ? "✗" : "✓"}</span>
        <div class="what">
          {#if !h.unreferenced_checked}
            Unused files were not checked.
          {:else if h.unreferenced.length}
            {plural(h.unreferenced.length, "file")} ({bytes(h.unreferenced_bytes)}) under Music that no track uses, including
            {plural(h.unreferenced.filter((f) => f.kind === "grid").length, "orphan beat grid")}.
          {:else}
            No unused files under Music.
          {/if}
        </div>
        {#if h.unreferenced_checked && h.unreferenced.length}
          <Button small disabled={device.blocked} onclick={() => (device.cleanupOpen = true)}>Review…</Button>
        {/if}
      </li>
    </ul>
    {#each h.warnings as w (w)}<p class="note">{w}</p>{/each}
  {/if}
</Card>

<style>
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  li {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    padding: 5px 0;
  }
  .what {
    flex: 1;
    min-width: 0;
  }
  .mark {
    width: 14px;
    flex: none;
    font-weight: 700;
  }
  .ok .mark {
    color: var(--ok);
  }
  .bad .mark {
    color: var(--bad);
  }
  .warn .mark {
    color: var(--accent);
  }
  .problems {
    margin: 4px 0 0;
    color: var(--bad-text);
    font-size: 12px;
  }
  .problems li {
    padding: 1px 0;
    display: list-item;
    list-style: disc inside;
  }
  .note {
    font-size: 12px;
    color: var(--muted);
    border-top: 1px solid var(--line);
    margin: 8px 0 0;
    padding-top: 8px;
  }
</style>
