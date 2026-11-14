<script lang="ts">
  import { session } from "$lib/state/session.svelte";
  import Button from "$lib/ui/Button.svelte";
  import WideModal from "../device/WideModal.svelte";
  import { ROW_CAP, ROW_MORE, sizeText } from "../sync/syncLogic";
  import { libs } from "./libraries.svelte";
  import { canCopy, copySummary, dialogSpace, dialogTitle, footerText, libraryNameFor, linesOf, type Line } from "./librariesLogic";

  // The question before a copy: every item the plan holds, with what is done to it and why, the size
  // and the room on the other side. Copy runs exactly this plan. A copy only adds to the target (the engine
  // clamps the Sync settings for it), and the footer is worded from the plan.
  const p = $derived(libs.pending);
  const lines = $derived(p ? linesOf(p.plan) : { acting: [], skipped: [], same: [] });
  const space = $derived(p ? dialogSpace(p.plan) : null);
  const ok = $derived(!!p && canCopy(p.plan));
  const targetName = $derived(p ? libraryNameFor(p.target, session.computer?.path ?? null) : "");
  const sourceName = $derived(p ? libraryNameFor(p.source, session.computer?.path ?? null) : "");

  // the tracks an "Add to case" row puts in, named (the first few, then how many more)
  const NAMES_SHOWN = 4;
  const namesText = (n: string[]) => n.slice(0, NAMES_SHOWN).join("; ") + (n.length > NAMES_SHOWN ? `; and ${n.length - NAMES_SHOWN} more` : "");
  // the plan note is about smart cases: only worth saying when a case is part of the copy
  const notes = $derived(p && p.plan.items.some((i) => i.kind === "case" && i.into_case === null) ? p.plan.notes : []);

  // the case a "Copy into a case" goes to, named even when its row is a quiet "already there"
  const intoName = $derived(p?.plan.items.find((i) => i.into_case !== null)?.into_case ?? null);

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
      From {sourceName} to {targetName}{#if intoName}, into the case “{intoName}”{/if}.
      {#if p.plan.bytes_to_copy > 0}<span class:bad={space.tone === "bad"} class:ok={space.tone === "ok"}>{space.text}</span>{/if}
    </p>
    {#if !space.fits}<p class="warn">It does not fit. Free some space on {targetName}, or pick fewer tracks.</p>{/if}

    <ul class="items" aria-label="What would be copied">
      {#each shown as l (l.key)}
        <li class:skip={!l.acts}>
          <span class="act" class:no={!l.acts}>{l.action}</span>
          <span class="name" title={l.name}>{l.name}</span>
          {#if l.acts && l.kind === "track" && l.bytes > 0}<span class="size muted">{sizeText(l.bytes)}</span>{/if}
          {#if l.why}<span class="why muted" title={l.why}>{l.why}</span>{/if}
          {#if l.names.length > 0}<span class="tracks muted" title={l.names.join(", ")}>{namesText(l.names)}</span>{/if}
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
    {#each notes as n (n)}<p class="muted note">{n}</p>{/each}
    <p class="muted note">{footerText(p.plan, targetName)}</p>

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
    flex-wrap: wrap;
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
    overflow-wrap: anywhere;
  }
  .tracks {
    flex: 1 1 100%;
    padding-left: 82px;
    font-size: 12px;
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
