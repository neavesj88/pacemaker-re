<script lang="ts">
  import { jobs } from "$lib/state/jobs.svelte";
  import Button from "$lib/ui/Button.svelte";
</script>

<div class="import">
  <section class="card">
    <h3>Music files</h3>
    <p class="muted">Drop files or folders anywhere in the window, or pick them. Each is analysed, copied to the device and added with its beat grid.</p>
    <Button variant="primary" onclick={() => jobs.pickFiles()} disabled={!jobs.canWrite}>Add tracks…</Button>
  </section>
  <section class="card">
    <h3>Other libraries</h3>
    <p class="muted">Importing playlists and cues from rekordbox, iTunes, Serato or Traktor is not built yet.</p>
  </section>
</div>

<style>
  .import {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(320px, 1fr));
    gap: 12px;
    padding: 14px;
  }
  .card {
    padding: 12px 14px;
    background: var(--panel);
    border: 1px solid var(--line);
    border-radius: 8px;
  }
  h3 {
    margin: 0 0 6px;
    font-size: 14px;
  }
</style>
