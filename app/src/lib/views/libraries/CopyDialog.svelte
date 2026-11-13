<script lang="ts">
  import Button from "$lib/ui/Button.svelte";
  import WideModal from "../device/WideModal.svelte";
  import { ROW_CAP, ROW_MORE, sizeText } from "../sync/syncLogic";
  import { libs } from "./libraries.svelte";
  import { canCopy, copySummary, dialogSpace, dialogTitle, libraryNameFor, linesOf, type Line } from "./librariesLogic";

  // The question before a copy: every item the plan holds, with what is done to it and why, the size
  // and the room on the other side. Copy runs exactly this plan. Nothing here can remove anything.
  const p = $derived(libs.pending);
  const lines = $derived(p ? linesOf(p.plan) : { acting: [], skipped: [], same: [] });
  const space = $derived(p ? dialogSpace(p.plan) : null);
  const ok = $derived(!!p && canCopy(p.plan));
  const targetName = $derived(p ? libraryNameFor(p.target) : "");
  const sourceName = $derived(p ? libraryNameFor(p.source) : "");

  let showSame = $state(false);
  let extra = $state(0);
  $effect(() => {
    void p;
    showSame = false;
    extra = 0;
  });

  // when everything is already there, say so item by item instead of hiding it all behind the button
  const sameShown = $derived(showSame || lines.acting.length + lines.skipped.length === 0);
  const list = $derived<Line[]>([...lines.acting, ...lines.skipped, ...(sameShown ? lines.same : [])]);
  const shown = $derived(list.slice(0, ROW_CAP + extra));
</script>

{#if p && space}
  <WideModal title={dialogTitle(targetName)} onclose={() => libs.cancelPending()} width={660}>
    <p class="sum"><b>{copySummary(p.plan)}</b></p>
    <p class="muted line">
      From {sourceName} to {targetName}.
      {#if p.plan.bytes_to_copy > 0}<span class:bad={space.tone === "bad"} class:ok={space.tone === "ok"}>{space.text}</span>{/if}
    </p>
    {#if !space.fits}<p class="warn">It does not fit. Free some space on {targetName}, or pick fewer tracks.</p>{/if}

    <ul class="items" aria-label="What would be copied">
      {#each shown as l (l.key)}
        <li class:skip={!l.acts}>
          <span class="act" class:no={!l.acts}>{l.action}</span>
          <span class="name" title={l.name}>{l.name}</span>
          {#if l.acts && l.kind === "track" && l.bytes > 0}<span class="size muted">{sizeText(l.bytes)}</span>{/if}
          {#if l.why}<span class="why muted">{l.why}</span>{/if}
        </li>
      {/each}
    </ul>
    <div class="more">
      {#if list.length > shown.length}
        <Button small onclick={() => (extra += ROW_MORE)}>Show {Math.min(ROW_MORE, list.length - shown.length)} more ({list.length - shown.length} not shown)</Button>
      {/if}
      {#if lines.same.length > 0 && lines.acting.length + lines.skipped.length > 0}
        <Button small variant="link" onclick={() => (showSame = !showSame)}>{showSame ? "Hide" : "Show"} the {lines.same.length} already there</Button>
      {/if}
    </div>
    {#each p.plan.notes as n (n)}<p class="muted note">{n}</p>{/each}
    <p class="muted note">Nothing is removed from either library. A copy of {targetName}'s database is saved before the first change.</p>

    {#snippet actions()}
      <Button data-autofocus onclick={() => libs.cancelPending()}>Cancel</Button>
      <Button variant="primary" disabled={!ok} onclick={() => libs.confirm()}>Copy</Button>
    {/snippet}
  </WideModal>
{/if}

<style>
  p {
    margin: 0 0 6px;
  }
  .sum {
    font-size: 14px;
  }
  .line {
    font-size: 12px;
  }
  .ok {
    color: var(--ok);
  }
  .bad,
  .warn {
    color: var(--bad);
  }
  .items {
    list-style: none;
    margin: 8px 0 0;
    padding: 0;
    max-height: 46vh;
    overflow: auto;
    border: 1px solid var(--line);
    border-radius: 4px;
  }
  li {
    display: flex;
    align-items: baseline;
    gap: 8px;
    padding: 4px 8px;
    border-bottom: 1px solid var(--line);
    font-size: 13px;
    min-width: 0;
  }
  li:last-child {
    border-bottom: none;
  }
  li.skip {
    opacity: 0.8;
  }
  .act {
    flex: none;
    min-width: 74px;
    color: var(--accent);
    font-size: 12px;
  }
  .act.no {
    color: var(--muted);
  }
  .name {
    flex: 1 1 auto;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .size {
    flex: none;
    font-size: 12px;
  }
  .why {
    flex: 0 1 auto;
    max-width: 45%;
    font-size: 12px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .more {
    display: flex;
    gap: 8px;
    margin-top: 6px;
  }
  .note {
    font-size: 12px;
    margin-top: 8px;
  }
</style>
