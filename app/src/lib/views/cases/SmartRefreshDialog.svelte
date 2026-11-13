<script lang="ts">
  import { session } from "$lib/state/session.svelte";
  // The question (or, from the button, the preview) before the Pacemaker copies of smart cases are
  // rewritten. Every smart case is named with what happens to its copy, and every track that
  // comes or goes is listed, so nothing is written that is not on this screen. The list is the
  // backend's own plan, the one the write is made from.
  import { jobs } from "$lib/state/jobs.svelte";
  import { smartRefresh } from "$lib/state/smartRefresh.svelte";
  import Button from "$lib/ui/Button.svelte";
  import Modal from "$lib/ui/Modal.svelte";
  import { changed, itemLine, itemTrackLines, plural, skipped } from "./smartRefreshLogic";

  const report = $derived(smartRefresh.report);
  const todo = $derived(report ? changed(report) : []);
  const stuck = $derived(report ? skipped(report) : []);
  const upToDate = $derived(report ? report.items.filter((i) => i.action === "unchanged") : []);
</script>

{#if report}
  <Modal title={`Refresh smart cases ${session.place.on}?`} onclose={() => smartRefresh.dismiss()}>
    {#if todo.length > 0}
      <p>
        The {session.isComputer ? 'case' : 'Pacemaker copy'} of {plural(todo.length, "smart case")} no longer matches {todo.length === 1 ? "its" : "their"} rules. Each copy is rewritten
        to hold exactly what the rules match now, in the order of the rules. Nothing else on the device is changed.
      </p>
      <ul class="items">
        {#each todo as i (i.id)}
          {@const lines = itemTrackLines(i)}
          <li>
            <div class="head">{itemLine(i, false)}</div>
            {#if i.renamed_from !== null}<div class="sub muted">Its name there is now “{i.renamed_from}”; it is named “{i.device_name}” again.</div>{/if}
            {#if i.reordered}<div class="sub muted">The tracks it keeps are put back in the order of the rules.</div>{/if}
            {#if lines.length > 0}
              <ul class="tracks" aria-label={`Tracks changing in ${i.name}`}>
                {#each lines as l, k (k)}
                  <li class={l.sign === "+" ? "add" : "rem"}>
                    <span class="sign" aria-hidden="true">{l.sign === "+" ? "+" : "−"}</span><span class="sr">{l.sign === "+" ? "Added:" : "Removed:"}</span>
                    {l.text}
                  </li>
                {/each}
              </ul>
            {/if}
          </li>
        {/each}
      </ul>
    {:else}
      <p>No {session.isComputer ? 'case' : 'Pacemaker copy'} needs rewriting.</p>
    {/if}
    {#if stuck.length > 0}
      <p class="warn">Cannot be refreshed, and left as they are:</p>
      <ul class="plain">
        {#each stuck as i (i.id)}<li>{itemLine(i, false)}</li>{/each}
      </ul>
    {/if}
    {#if upToDate.length > 0}
      <p class="muted">Already up to date: {upToDate.map((i) => `“${i.name}”`).join(", ")}.</p>
    {/if}
    {#snippet actions()}
      <Button data-autofocus onclick={() => smartRefresh.dismiss()}>{todo.length > 0 ? "Not now" : "Close"}</Button>
      {#if todo.length > 0}
        <Button variant="primary" disabled={!jobs.canWrite || smartRefresh.applying} onclick={() => smartRefresh.confirm()}>
          Refresh {plural(todo.length, "smart case")}
        </Button>
      {/if}
    {/snippet}
  </Modal>
{/if}

<style>
  p {
    margin: 0 0 8px;
  }
  .items,
  .plain {
    margin: 0 0 8px;
    padding: 6px 8px 6px 22px;
    max-height: 340px;
    overflow: auto;
    background: var(--deep);
    border: 1px solid var(--line);
    border-radius: 4px;
  }
  .items > li {
    margin-bottom: 8px;
  }
  .head {
    font-weight: 600;
  }
  .sub {
    font-size: 12px;
  }
  .tracks {
    margin: 4px 0 0;
    padding: 0;
    list-style: none;
    max-height: 150px;
    overflow: auto;
    font-size: 12px;
  }
  .tracks li {
    padding: 1px 0;
  }
  .add .sign {
    color: var(--ok);
  }
  .rem .sign {
    color: var(--bad);
  }
  .sign {
    display: inline-block;
    width: 1em;
    font-weight: 700;
  }
  .sr {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip-path: inset(50%);
  }
  .warn {
    color: var(--bad-text);
  }
</style>
