<script lang="ts">
  import { deviceSpaceRun, JOB_SPACE } from "$lib/api/device";
  import { session } from "$lib/state/session.svelte";
  import { toast } from "$lib/state/toast.svelte";
  import Button from "$lib/ui/Button.svelte";
  import { device } from "../device/device.svelte";
  import WideModal from "../device/WideModal.svelte";
  import { space } from "./space.svelte";
  import { bytesText, duplicateConflicts, needsAcknowledge, plural, runButton, runFlags, runLine, runTitle } from "./spaceLogic";

  // The confirmation of an archive and/or removal. Every track is listed with what will happen to
  // it, taken from `deviceSpaceRunPreview`: the same check the run repeats on the fresh database
  // before it acts. Nothing starts until the button here is pressed.
  let c = $derived(space.confirm);
  let acknowledged = $state(false);
  let starting = $state(false);

  // a new confirmation starts unticked
  $effect(() => {
    void c;
    acknowledged = false;
  });

  const conflicts = $derived(c ? duplicateConflicts(space.items, new Set(c.ids)) : []);
  const flags = $derived(c ? runFlags(c.mode) : { archive: false, remove: false });
  const staying = $derived(c ? c.plan.items.filter((i) => !i.remove && flags.remove) : []);
  const ack = $derived(c ? needsAcknowledge(c.plan, c.mode) : false);
  const blocked = $derived(!c || c.plan.problems.length > 0 || starting || (ack && !acknowledged));

  async function go() {
    const lib = session.library;
    const conf = space.confirm;
    if (!lib || !conf || blocked) return;
    starting = true;
    const f = runFlags(conf.mode);
    device.begin(JOB_SPACE, f.archive ? "Starting the archive" : "Starting the removal");
    try {
      await space.flushSettings();
      await deviceSpaceRun(lib.root, conf.ids, {
        archive_dir: f.archive ? space.settings.archive_dir : null,
        include_grids: space.settings.include_grids,
        remove: f.remove,
        require_archive: space.settings.require_archive,
      });
      space.closeConfirm();
    } catch (e) {
      device.job = null;
      toast.error("Couldn't run the space cleanup.", e);
    } finally {
      starting = false;
    }
  }
</script>

{#if c}
  {@const conf = c}
  <WideModal title={runTitle(conf.plan, conf.mode)} onclose={() => space.closeConfirm()} width={760}>
    {#if flags.archive}
      <p>
        Each audio file{space.settings.include_grids ? " and beat grid" : ""} is copied, read back and compared in
        <code class="path">{space.settings.archive_dir}</code>.
        {#if conf.plan.archive_free_bytes !== null}
          Needs {bytesText(conf.plan.archive_bytes)}; {bytesText(conf.plan.archive_free_bytes)} free.
        {:else}
          Needs {bytesText(conf.plan.archive_bytes)}.
        {/if}
      </p>
    {/if}
    {#if flags.remove}
      <p class="danger">
        {#if flags.archive}
          Removed only after its copy is verified; a failed copy stays.
        {:else}
          <b>No copy is made.</b>
        {/if}
        Deletes the audio and beat grid from the card for good. Restore brings back the rows, not the audio.
      </p>
    {/if}

    {#if conf.plan.problems.length > 0}
      <ul class="problems" role="alert">
        {#each conf.plan.problems as p (p)}<li>{p}</li>{/each}
      </ul>
    {/if}
    {#if conflicts.length > 0}
      <div class="problems" role="alert">
        <b>No copy of this audio would be left on the card:</b>
        <ul>
          {#each conflicts as k (k.keeper)}
            <li>{k.keeper} is ticked together with {plural(k.copies.length, "copy", "copies")} of it ({k.copies.join(", ")}).</li>
          {/each}
        </ul>
        {#if !flags.archive}Untick the one you want to keep.{/if}
      </div>
    {/if}

    <ul class="files" aria-label="Tracks this will act on">
      {#each conf.plan.items as i (i.track_id)}
        <li class:stays={flags.remove && !i.remove}>
          <span class="name">{i.label}</span>
          <span class="size num muted">{bytesText(i.bytes)}</span>
          <span class="what muted">{runLine(i)}</span>
        </li>
      {/each}
    </ul>
    {#if conf.plan.skipped.length > 0}
      <p class="muted">Not acted on:</p>
      <ul class="skipped">
        {#each conf.plan.skipped as s (s)}<li>{s}</li>{/each}
      </ul>
    {/if}
    {#if flags.remove && staying.length > 0}
      <p class="muted">{plural(staying.length, "track")} stay on the card (listed above with the reason).</p>
    {/if}

    {#if ack}
      <label class="ack">
        <input type="checkbox" bind:checked={acknowledged} />
        I understand the {plural(conf.plan.items.filter((i) => i.remove).length, "track")} listed above will be deleted from the card with no copy.
      </label>
    {/if}

    {#snippet actions()}
      <Button data-autofocus onclick={() => space.closeConfirm()}>Cancel</Button>
      <Button variant="primary" disabled={blocked} onclick={go}>{starting ? "Starting…" : runButton(conf.plan, conf.mode)}</Button>
    {/snippet}
  </WideModal>
{/if}

<style>
  p {
    margin: 0 0 10px;
  }
  .path {
    overflow-wrap: anywhere;
  }
  .danger {
    padding: 8px 10px;
    border: 1px solid var(--bad);
    border-radius: 4px;
    background: var(--bad-bg);
    color: var(--bad-text);
  }
  .problems {
    margin: 0 0 10px;
    padding: 8px 10px 8px 26px;
    border: 1px solid var(--bad);
    border-radius: 4px;
    background: var(--bad-bg);
    color: var(--bad-text);
  }
  div.problems {
    padding-left: 10px;
  }
  div.problems ul {
    margin: 4px 0 0;
    padding-left: 18px;
  }
  .files,
  .skipped {
    list-style: none;
    margin: 0 0 10px;
    padding: 0;
    max-height: 300px;
    overflow: auto;
    border: 1px solid var(--line);
    border-radius: 4px;
  }
  .skipped {
    max-height: 120px;
    padding: 6px 8px;
    color: var(--muted);
  }
  .files li {
    display: grid;
    grid-template-columns: 1fr auto;
    gap: 0 12px;
    padding: 4px 8px;
  }
  .files li:nth-child(odd) {
    background: var(--row);
  }
  .name {
    overflow-wrap: anywhere;
  }
  .what {
    grid-column: 1 / -1;
    font-size: 12px;
    overflow-wrap: anywhere;
  }
  .stays .name {
    color: var(--muted);
  }
  .ack {
    display: flex;
    gap: 8px;
    align-items: flex-start;
    margin-top: 6px;
    color: var(--bad-text);
  }
  .ack input {
    padding: 0;
    margin-top: 2px;
    accent-color: var(--accent);
  }
</style>
