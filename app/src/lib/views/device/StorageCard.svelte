<script lang="ts">
  import { nav } from "$lib/state/nav.svelte";
  import { session } from "$lib/state/session.svelte";
  import Button from "$lib/ui/Button.svelte";
  import Card from "./Card.svelte";
  import { device } from "./device.svelte";
  import { bytes } from "./format";

  let s = $derived(device.overview?.storage);
  let lib = $derived(session.library);
  let unused = $derived(device.health?.unreferenced_checked ? device.health.unreferenced.length : 0);

  let meterLabel = $derived(
    s
      ? s.total_bytes > 0
        ? `Storage use: ${bytes(s.music_bytes)} music, ${bytes(s.grids_mixes_bytes)} grids and mixes, ${bytes(s.other_bytes)} other, ${bytes(s.free_bytes)} free of ${bytes(s.total_bytes)}`
        : `Storage use: ${bytes(s.music_bytes)} music, ${bytes(s.grids_mixes_bytes)} grids and mixes; free space unknown`
      : "Storage use",
  );

  // a non-empty segment is never thinner than a sliver, so small use stays visible
  const pct = (n: number) => (s && s.total_bytes > 0 ? Math.max((n / s.total_bytes) * 100, n > 0 ? 0.6 : 0) : 0);
</script>

<Card title="Storage">
  {#if !s}
    <p class="muted">Reading the drive…</p>
  {:else}
    <div class="meter" role="img" aria-label={meterLabel}>
      <i class="music" style="width:{pct(s.music_bytes)}%"></i>
      <i class="grids" style="width:{pct(s.grids_mixes_bytes)}%"></i>
      <i class="other" style="width:{pct(s.other_bytes)}%"></i>
    </div>
    {#if s.total_bytes > 0}
      <p class="legend muted">
        <span><b class="sw music"></b>{bytes(s.music_bytes)} music</span>
        <span><b class="sw grids"></b>{bytes(s.grids_mixes_bytes)} grids and mixes</span>
        {#if s.other_bytes > 0}<span><b class="sw other"></b>{bytes(s.other_bytes)} other</span>{/if}
        <span class="free">{bytes(s.free_bytes)} free of {bytes(s.total_bytes)}</span>
      </p>
    {:else}
      <p class="muted">
        {bytes(s.music_bytes)} music, {bytes(s.grids_mixes_bytes)} grids and mixes. Free space could not be read.
      </p>
    {/if}
  {/if}
  {#if lib}
    <div class="field"><span class="k">Tracks</span><span>{lib.tracks.length}</span></div>
    <div class="field"><span class="k">Mixes</span><span>{lib.mixes.length}</span></div>
    <div class="field"><span class="k">Cases</span><span>{lib.cases.length}</span></div>
  {/if}
  <div class="actions">
    <Button
      disabled={!device.health?.unreferenced_checked || unused === 0 || device.blocked}
      onclick={() => (device.cleanupOpen = true)}
    >
      {unused > 0 ? `Clean up ${unused} unused file${unused === 1 ? "" : "s"}…` : "Clean up…"}
    </Button>
    <Button disabled={!lib} onclick={() => nav.go("space")}>Free up space…</Button>
  </div>
</Card>

<style>
  .meter {
    height: 12px;
    border-radius: 6px;
    background: var(--row-alt);
    overflow: hidden;
    display: flex;
  }
  .meter i {
    display: block;
    height: 100%;
  }
  .music {
    background: var(--accent);
  }
  .grids {
    background: var(--text);
  }
  .other {
    background: var(--muted);
  }
  .legend {
    display: flex;
    flex-wrap: wrap;
    gap: 4px 14px;
    font-size: 12px;
    margin: 8px 0;
  }
  .sw {
    display: inline-block;
    width: 10px;
    height: 10px;
    border-radius: 2px;
    margin-right: 5px;
    vertical-align: middle;
  }
  .free {
    color: var(--green);
  }
  .field {
    display: grid;
    grid-template-columns: 96px 1fr;
    gap: 6px;
    margin: 4px 0;
  }
  .k {
    color: var(--muted);
    font-size: 12px;
  }
  .actions {
    margin-top: 10px;
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }
</style>
