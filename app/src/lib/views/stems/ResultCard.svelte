<script lang="ts">
  import { session } from "$lib/state/session.svelte";
  import { jobs } from "$lib/state/jobs.svelte";
  import { stems } from "$lib/state/stems.svelte";
  import Button from "$lib/ui/Button.svelte";
  import { clock, fileName, noun, resultFiles } from "./logic";

  // What the last run made. The files are on this computer; the button hands them to the ordinary
  // "add tracks" step, which names every file and asks before anything is copied onto the card.
  const r = $derived(stems.result);
  const files = $derived(resultFiles(r));
</script>

{#if r}
  <section class="card">
    <header>
      <h4>{r.cancelled ? "Stopped" : "Finished"}</h4>
      <Button small variant="ghost" onclick={() => (stems.result = null)}>Dismiss</Button>
    </header>

    <p class="sum">
      {noun(r.written.length, "track")} separated on {r.device}, saved in <code>{r.dest}</code>.
      {#if r.cancelled}Files of finished tracks were kept; nothing half-written was left behind.{/if}
    </p>

    {#if r.limited_samples > 0}
      <p class="note muted">{r.limited_samples.toLocaleString()} samples were limited to full scale to avoid clipping.</p>
    {/if}

    {#if r.written.length}
      <ul class="done">
        {#each r.written as w (w.track_id)}
          <li>
            <strong>{w.label}</strong>
            <span class="muted num">{clock(w.seconds)}</span>
            <div class="files muted">
              {#if w.vocals}{fileName(w.vocals)}{/if}{#if w.vocals && w.instrumental}<br />{/if}{#if w.instrumental}{fileName(w.instrumental)}{/if}
            </div>
          </li>
        {/each}
      </ul>
    {/if}

    {#if r.failed.length}
      <h5>Failed</h5>
      <ul class="bad">
        {#each r.failed as f (f.label)}<li><strong>{f.label}</strong>: {f.error}</li>{/each}
      </ul>
    {/if}
    {#if r.skipped.length}
      <h5>Left alone</h5>
      <ul class="muted">
        {#each r.skipped as f (f.label)}<li><strong>{f.label}</strong>: {f.error}</li>{/each}
      </ul>
    {/if}

    {#if files.length}
      <div class="add">
        <span class="muted">Put {files.length === 1 ? "this file" : `these ${files.length} files`} {session.isComputer ? 'in the library' : 'on the Pacemaker'} too?</span>
        <Button disabled={!jobs.canWrite || stems.busy} title={jobs.canWrite ? undefined : "The library is read-only or a job is running"} onclick={() => stems.addResultsToCard(files)}>
          Add {noun(files.length, "file")} {session.isComputer ? 'to the library' : 'to the card'}…
        </Button>
      </div>
      <p class="muted note">You are shown every file and asked before anything is copied. The library does not keep tags or a link to the original; they appear as ordinary tracks.</p>
    {/if}
  </section>
{/if}

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
    margin-bottom: 6px;
  }
  h4 {
    margin: 0;
    font-size: 13px;
  }
  h5 {
    margin: 10px 0 2px;
    font-size: 12px;
  }
  .sum {
    margin: 0;
    overflow-wrap: anywhere;
  }
  .note {
    font-size: 12px;
    margin: 6px 0 0;
  }
  ul {
    margin: 4px 0 0;
    padding-left: 0;
    list-style: none;
    font-size: 12px;
  }
  .done {
    max-height: 220px;
    overflow: auto;
    border: 1px solid var(--line);
    border-radius: 4px;
    background: var(--deep);
  }
  .done li {
    padding: 4px 10px;
    border-bottom: 1px solid var(--line);
  }
  .done li:last-child {
    border-bottom: none;
  }
  .files {
    font-size: 11px;
    overflow-wrap: anywhere;
  }
  .bad {
    color: var(--bad-text);
  }
  .add {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    margin-top: 12px;
  }
</style>
