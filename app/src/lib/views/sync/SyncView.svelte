<script lang="ts">
  import { JOB_SYNC } from "$lib/api/sync";
  import { nav } from "$lib/state/nav.svelte";
  import { sameRoot } from "$lib/state/place";
  import { session } from "$lib/state/session.svelte";
  import Button from "$lib/ui/Button.svelte";
  import { device } from "../device/device.svelte";
  import JobProgress from "../device/JobProgress.svelte";
  import WideModal from "../device/WideModal.svelte";
  import SyncSettingsFields from "./SyncSettingsFields.svelte";
  import { sync } from "./sync.svelte";
  import {
    actionText, defaultPair, dependencyMarks, doesAnything, GROUPS, groupItems, isActionable, isRemoval, isSame, libraryOptions, markText,
    pairFor, pairReady, pickSide, plural, reasonText, removalList, resultParts, ROW_CAP, ROW_MORE, sizeText, spaceLine, swapPair, tally, tallyText,
    type GroupId,
  } from "./syncLogic";

  // Move tracks, mixes and cases between the library on this computer and a Pacemaker, either way.
  // "Check what would change" only reads; the list it shows is what a run does (the run is refused if
  // a library moved in between). Every item is named, can be left out, and nothing is removed unless
  // the setting for it is on and the list was confirmed.

  const options = $derived(libraryOptions(session.computer, session.volumes, session.library));
  const open = $derived(session.library ? { root: session.library.root, kind: session.library.kind } : null);
  const hasPacemaker = $derived(options.some((o) => o.kind === "pacemaker"));
  const hasComputer = $derived(options.some((o) => o.kind === "computer"));
  const nameOf = (root: string) => options.find((o) => sameRoot(o.value, root))?.label ?? root;
  /** The name inside a sentence: "this computer", "Pacemaker (G:)". */
  const inText = (root: string) => nameOf(root).replace(/^This computer$/, "this computer");

  const pair = $derived(sync.pair);
  const ready = $derived(pairReady(pair));

  // The pair: a link from another view wins; else the first visit chooses; later the pair is kept
  // unless a side is no longer on offer (a Pacemaker was unplugged).
  $effect(() => {
    const dir = nav.params.direction;
    const other = nav.params.other;
    let want: { from: string; to: string } | null = null;
    if (dir) {
      nav.take("direction");
      nav.take("other");
      want = defaultPair(options, open, { direction: dir, other });
    } else {
      const there = (r: string) => !!r && options.some((o) => sameRoot(o.value, r));
      if (!sync.pairSet || !(there(sync.pair.from) && there(sync.pair.to))) want = defaultPair(options, open);
    }
    if (want && (want.from !== sync.pair.from || want.to !== sync.pair.to || !sync.pairSet)) sync.setPair(want);
  });

  $effect(() => {
    void session.refreshVolumes().catch(() => {});
  });

  const plan = $derived(sync.plan);
  const groups = $derived(plan ? groupItems(plan.items) : null);
  const marks = $derived(plan ? dependencyMarks(plan.items, sync.excluded) : new Map());
  const t = $derived(plan ? tally(plan.items, sync.excluded) : null);
  const space = $derived(plan && t ? spaceLine(plan, t.bytes) : null);
  const removing = $derived(plan ? removalList(plan.items, sync.excluded) : []);
  /** Mixes and cases whose contents on the target are overwritten by the ticked plan, named in words. */
  const replacing = $derived(plan ? plan.items.filter((i) => i.action.type === "replace" && !sync.excluded.has(i.key) && !marks.get(i.key)?.skipped).map((i) => `${i.kind === "mix" ? "mix" : "case"} "${i.label}"`) : []);
  const canStart = $derived(!!plan && !!t && doesAnything(t) && !!space?.fits && !sync.busy && !sync.stale);

  let showSame = $state<Record<string, boolean>>({});
  /** Rows added beyond the first ROW_CAP, per group, by "Show more". */
  let extra = $state<Record<string, number>>({});
  let confirming = $state(false);
  $effect(() => {
    // a new plan starts with short lists
    void sync.plan;
    showSame = {};
    extra = {};
    confirming = false;
  });

  function pick(side: "from" | "to", value: string) {
    sync.setPair(pickSide(side, value, pair, options));
  }
  const quick = (dir: "send" | "get") => sync.setPair(pairFor(dir, options, pair));

  function start() {
    if (!canStart) return;
    if (removing.length > 0) confirming = true;
    else void sync.apply();
  }

  function confirmed() {
    confirming = false;
    void sync.apply();
  }

  const keysOf = (g: GroupId) => (groups?.[g] ?? []).filter(isActionable).map((i) => i.key);
  const parts = $derived(sync.result ? resultParts(sync.result.summary) : []);
</script>

<div class="page">
  <h2>Sync</h2>
  <p class="muted lead">
    Copy tracks, mixes and cases from one library to the other, either way. You see exactly what would change before anything is
    written, and you can leave any item out.
  </p>

  {#if !hasPacemaker}
    <section class="card" aria-label="No Pacemaker found">
      <h3>No Pacemaker found</h3>
      <p>
        Plug the Pacemaker into this computer over USB, or put its card in a card reader on this PC. It appears as a drive;
        then press Rescan. If its library is in a folder, open that folder instead.
      </p>
      <div class="actions">
        <Button variant="primary" onclick={() => session.refreshVolumes()}>Rescan</Button>
        <Button onclick={() => session.openFolder()}>Open folder…</Button>
      </div>
    </section>
  {/if}

  {#if options.length >= 2}
    <section class="card" aria-label="Choose the two libraries">
      <div class="pair">
        <label class="side">
          <span class="k">From</span>
          <select value={pair.from} onchange={(e) => pick("from", e.currentTarget.value)} disabled={sync.busy}>
            <option value="" disabled>Choose a library…</option>
            {#each options as o (o.value)}<option value={o.value}>{o.label}</option>{/each}
          </select>
        </label>
        <Button title="Swap the two" aria-label="Swap From and To" disabled={sync.busy} onclick={() => sync.setPair(swapPair(pair))}>⇄</Button>
        <label class="side">
          <span class="k">To</span>
          <select value={pair.to} onchange={(e) => pick("to", e.currentTarget.value)} disabled={sync.busy}>
            <option value="" disabled>Choose a library…</option>
            {#each options as o (o.value)}<option value={o.value}>{o.label}</option>{/each}
          </select>
        </label>
      </div>
      <div class="actions">
        <Button disabled={sync.busy || !hasPacemaker || !hasComputer} onclick={() => quick("send")}>Send to Pacemaker</Button>
        <Button disabled={sync.busy || !hasPacemaker || !hasComputer} onclick={() => quick("get")}>Get from Pacemaker</Button>
        <span class="grow"></span>
        <Button small variant="ghost" onclick={() => session.refreshVolumes()} title="Look for a connected Pacemaker">Rescan</Button>
        <Button small variant="ghost" onclick={() => session.openFolder()}>Open folder…</Button>
      </div>
      {#if pair.from && pair.to && sameRoot(pair.from, pair.to)}
        <p class="warn">Choose two different libraries.</p>
      {/if}
      <div class="actions">
        <Button variant="primary" disabled={!ready || sync.planning || sync.busy} onclick={() => sync.check()}>
          {sync.planning ? "Checking…" : "Check what would change"}
        </Button>
        {#if ready}<span class="muted">{nameOf(pair.from)} to {nameOf(pair.to)}</span>{/if}
      </div>
    </section>
  {/if}

  <details class="card settings">
    <summary>Sync settings</summary>
    <SyncSettingsFields />
  </details>

  {#if sync.error}
    <p class="banner" role="alert">
      {sync.error}
      {#if sync.stale}<Button small onclick={() => sync.check()}>Check again</Button>{/if}
    </p>
  {/if}

  {#if sync.running || (device.job?.kind === JOB_SYNC)}
    <section class="card" aria-label="Sync progress">
      <h3>Syncing…</h3>
      <JobProgress kind={JOB_SYNC} />
    </section>
  {/if}

  {#if sync.result}
    {@const r = sync.result}
    <section class="card" aria-label="Result">
      <h3>{r.failed > 0 ? "Finished, with problems" : "Finished"}</h3>
      <ul class="result">
        {#each parts as p, n (n)}
          <li class={p.tone}>
            {p.heading}{p.items.length ? ":" : ""}
            {#if p.items.length}
              <ul>{#each p.items as it, k (k)}<li>{it}</li>{/each}</ul>
            {/if}
          </li>
        {/each}
      </ul>
      <div class="actions"><Button onclick={() => sync.check()} disabled={!ready || sync.busy}>Check again</Button></div>
    </section>
  {/if}

  {#if plan && groups && t && space}
    <section class="card" aria-label="What would change">
      <div class="bar">
        <div class="sum">
          <b>{tallyText(t)}</b>
          <span class:bad={space.tone === "bad"} class:ok={space.tone === "ok"} class="muted space">{space.text}</span>
        </div>
        <Button variant="primary" disabled={!canStart} onclick={start}>
          {removing.length > 0 ? `Sync and remove ${plural(removing.length, "item")}` : "Sync"}
        </Button>
      </div>
      {#each plan.notes as n (n)}<p class="muted note">{n}</p>{/each}
      {#if replacing.length > 0}<p class="muted note">Replaced on {inText(sync.planFor?.to ?? "")}: {replacing.join(", ")}.</p>{/if}
      <p class="muted note">A copy of {inText(sync.planFor?.to ?? "")}'s database is saved before the first change, so Restore can bring the rows back. Copied audio files stay.</p>
      {#if !space.fits}<p class="warn">Leave some tracks out, or free space on {nameOf(sync.planFor?.to ?? "")}, to continue.</p>{/if}

      {#each GROUPS as g (g.id)}
        {@const all = groups[g.id]}
        {#if all.length > 0}
          {@const act = all.filter(isActionable)}
          {@const same = all.filter(isSame)}
          {@const list = showSame[g.id] ? all : all.filter((i) => !isSame(i))}
          {@const shown = list.slice(0, ROW_CAP + (extra[g.id] ?? 0))}
          <div class="group" class:removals={g.id === "removals"}>
            <div class="ghead">
              <h3>{g.id === "removals" ? "Removals: deleted from " + inText(sync.planFor?.to ?? "") : g.title}</h3>
              <span class="muted">{act.length} to do{same.length ? `, ${same.length} already the same` : ""}</span>
              {#if act.length > 0}
                <span class="grow"></span>
                <Button small variant="link" onclick={() => sync.setMany(keysOf(g.id), true)}>Tick all</Button>
                <Button small variant="link" onclick={() => sync.setMany(keysOf(g.id), false)}>Leave all out</Button>
              {/if}
            </div>
            {#if g.id === "removals"}
              <p class="warn">
                These exist on {inText(sync.planFor?.to ?? "")} and not on {inText(sync.planFor?.from ?? "")}. A tick means
                the item is deleted. You are asked to confirm the list before anything is.
              </p>
            {/if}
            <ul class="items">
              {#each shown as i (i.key)}
                {@const m = marks.get(i.key)}
                {@const left = sync.excluded.has(i.key)}
                <li class:off={left || !isActionable(i)} class:bad={g.id === "removals" && isActionable(i) && !left}>
                  <label class="top">
                    {#if isActionable(i)}
                      <input type="checkbox" checked={!left} onchange={() => sync.toggle(i.key)} aria-label={`${actionText(i.action)} ${i.label}`} />
                    {:else}
                      <span class="nobox" aria-hidden="true"></span>
                    {/if}
                    <span class="name">{i.label}</span>
                    {#if i.bytes > 0 && i.kind === "track" && isActionable(i)}<span class="size muted">{sizeText(i.bytes)}</span>{/if}
                  </label>
                  <div class="why">
                    <span class="act" class:skip={!isActionable(i)}>{actionText(i.action)}{isActionable(i) ? ":" : ""}</span>
                    {#if isActionable(i)}<span class="muted">{reasonText(i, plan.settings, plan.source_label)}</span>{/if}
                    {#if i.detail}<span class="muted"> ({i.detail})</span>{/if}
                  </div>
                  {#if left && isActionable(i)}<div class="why muted">Left out: nothing is done with it.</div>{/if}
                  {#if m && !left}<div class="why mark">{markText(i.kind, isRemoval(i), m)}</div>{/if}
                </li>
              {/each}
            </ul>
            <div class="more">
              {#if list.length > shown.length}
                <Button small onclick={() => (extra[g.id] = (extra[g.id] ?? 0) + ROW_MORE)}>
                  Show {Math.min(ROW_MORE, list.length - shown.length)} more ({list.length - shown.length} not shown)
                </Button>
              {/if}
              {#if same.length > 0}
                <Button small variant="link" onclick={() => (showSame[g.id] = !showSame[g.id])}>
                  {showSame[g.id] ? "Hide" : "Show"} the {same.length} already the same
                </Button>
              {/if}
            </div>
          </div>
        {/if}
      {/each}

      {#if plan.items.length === 0 || !doesAnything(t)}
        <p class="muted">Nothing to do: the two libraries already match, or everything is left out.</p>
      {/if}
    </section>
  {/if}
</div>

{#if confirming}
  <WideModal title={`Remove ${plural(removing.length, "item")} from ${nameOf(sync.planFor?.to ?? "")}?`} onclose={() => (confirming = false)} width={640}>
    <p class="warn">
      These are deleted from {nameOf(sync.planFor?.to ?? "")} because {inText(sync.planFor?.from ?? "")} does not have them. A copy of
      its database is saved first, so Restore can bring the rows back; deleted audio files are not kept.
    </p>
    <ul class="confirm">
      {#each removing as r, n (n)}<li><span class="muted">{r.kind}</span> {r.label}</li>{/each}
    </ul>
    <p class="muted">Everything else in the list is copied as you saw it.</p>
    {#snippet actions()}
      <Button data-autofocus onclick={() => (confirming = false)}>Cancel</Button>
      <Button variant="primary" onclick={confirmed}>Sync and remove {plural(removing.length, "item")}</Button>
    {/snippet}
  </WideModal>
{/if}

<style>
  .page {
    padding: 14px 18px 40px;
    display: flex;
    flex-direction: column;
    gap: 12px;
    max-width: 980px;
    min-width: 0;
    overflow: auto;
    height: 100%;
    box-sizing: border-box;
  }
  h2 {
    margin: 0;
  }
  h3 {
    margin: 0;
    font-size: 14px;
  }
  .lead {
    margin: 0;
  }
  .card {
    border: 1px solid var(--line);
    border-radius: 6px;
    background: var(--panel);
    padding: 12px 14px;
    display: flex;
    flex-direction: column;
    gap: 10px;
    min-width: 0;
  }
  .card p {
    margin: 0;
  }
  .pair {
    display: flex;
    align-items: flex-end;
    gap: 10px;
    flex-wrap: wrap;
  }
  .side {
    display: flex;
    flex-direction: column;
    gap: 3px;
    flex: 1 1 220px;
    min-width: 0;
  }
  .side select {
    min-width: 0;
    width: 100%;
  }
  .k {
    font-size: 11px;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--muted);
  }
  .actions {
    display: flex;
    gap: 8px;
    align-items: center;
    flex-wrap: wrap;
  }
  .grow {
    flex: 1;
  }
  .settings summary {
    cursor: pointer;
    font-weight: 600;
  }
  .settings[open] summary {
    margin-bottom: 10px;
  }
  .banner {
    margin: 0;
    padding: 8px 12px;
    border: 1px solid var(--bad);
    border-radius: 6px;
    background: var(--bad-bg);
    color: var(--bad-text);
    display: flex;
    gap: 10px;
    align-items: center;
    flex-wrap: wrap;
  }
  .warn {
    color: var(--bad-text);
  }
  .bar {
    display: flex;
    gap: 12px;
    align-items: center;
    justify-content: space-between;
    flex-wrap: wrap;
    position: sticky;
    top: -14px;
    z-index: 2;
    background: var(--panel);
    padding: 8px 0;
    border-bottom: 1px solid var(--line);
  }
  .sum {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }
  .space.ok {
    color: var(--ok);
  }
  .space.bad {
    color: var(--bad-text);
    font-weight: 600;
  }
  .note {
    font-size: 12px;
  }
  .group {
    border: 1px solid var(--line);
    border-radius: 6px;
    padding: 8px 10px;
    display: flex;
    flex-direction: column;
    gap: 6px;
    min-width: 0;
  }
  .group.removals {
    border-color: var(--bad);
    background: var(--bad-bg);
  }
  .group.removals h3 {
    color: var(--bad-text);
  }
  .ghead {
    display: flex;
    gap: 10px;
    align-items: baseline;
    flex-wrap: wrap;
  }
  .items {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
  }
  .items > li {
    padding: 5px 0;
    border-top: 1px solid var(--line);
    min-width: 0;
  }
  .items > li:first-child {
    border-top: none;
  }
  .items > li.off .name {
    color: var(--muted);
  }
  .top {
    display: flex;
    gap: 8px;
    align-items: baseline;
    min-width: 0;
  }
  .top input {
    margin: 0;
    flex: none;
    accent-color: var(--accent);
  }
  .nobox {
    width: 13px;
    flex: none;
  }
  .name {
    flex: 1;
    min-width: 0;
    overflow-wrap: anywhere;
  }
  .size {
    flex: none;
    font-variant-numeric: tabular-nums;
  }
  .why {
    margin-left: 21px;
    font-size: 12px;
    overflow-wrap: anywhere;
  }
  .act {
    font-weight: 600;
  }
  .act.skip {
    font-weight: 400;
    color: var(--muted);
  }
  li.bad .act {
    color: var(--bad-text);
  }
  .mark {
    color: var(--cue);
  }
  .more {
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
  }
  .result {
    margin: 0;
    padding-left: 18px;
  }
  .result ul {
    margin: 2px 0 0;
    padding-left: 18px;
  }
  .result li.bad {
    color: var(--bad-text);
  }
  .result li.muted {
    color: var(--muted);
  }
  .confirm {
    margin: 8px 0;
    padding-left: 18px;
    max-height: 46vh;
    overflow: auto;
    overflow-wrap: anywhere;
  }
</style>
