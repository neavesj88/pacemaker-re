<script lang="ts">
  // While tracks are being dragged (from the Tracks table or from rows of an open case), this
  // strip lists the cases on the left of the window so they can be dropped on one. Tracks and
  // Cases are separate views, so without it there would be nothing to drop on. It floats over
  // the sidebar's lower part (no layout shift under the cursor) and exists only during a drag.
  import { cases } from "$lib/state/cases.svelte";
  import { jobs } from "$lib/state/jobs.svelte";
  import { toast } from "$lib/state/toast.svelte";
  import { endTrackDrag, hasTrackDrag, readTrackDrag, trackDrag } from "$lib/state/trackDrag.svelte";
  import { plural } from "./caseLogic";
  import NameDialog from "./NameDialog.svelte";

  let over = $state<number | "new" | null>(null);
  let lastOver = Date.now();
  let newFrom = $state<number[] | null>(null);

  // the device copy of a smart case is rewritten from its rules on every refresh, so tracks
  // dropped on it would not last
  const list = $derived(cases.infos.filter((c) => !cases.smart.some((s) => s.device_case_id === c.id)));
  const openId = $derived(cases.open?.id ?? null);

  function allow(e: DragEvent, target: number | "new") {
    if (!hasTrackDrag(e) || !jobs.canWrite) return;
    e.preventDefault();
    if (e.dataTransfer) e.dataTransfer.dropEffect = "copy";
    over = target;
  }

  async function drop(e: DragEvent, target: number | "new") {
    over = null;
    const ids = readTrackDrag(e);
    endTrackDrag();
    if (ids.length === 0 || !jobs.canWrite) return;
    e.preventDefault();
    if (target === "new") newFrom = ids;
    else {
      const c = list.find((x) => x.id === target);
      if (c) await cases.dropTracks(c.id, c.name, ids);
    }
  }

  // A drag that ends where nothing reports it (its row was scrolled away, the window lost it)
  // would leave the strip up: dragover fires all the time during a drag, so silence ends it.
  $effect(() => {
    if (!trackDrag.active) return;
    lastOver = Date.now();
    const watchdog = setInterval(() => {
      if (Date.now() - lastOver > 2500) endTrackDrag();
    }, 1000);
    return () => clearInterval(watchdog);
  });
</script>

<svelte:window
  ondragover={() => (lastOver = Date.now())}
  ondragend={endTrackDrag}
  ondrop={() => {
    over = null;
    endTrackDrag();
  }}
/>

{#if trackDrag.active}
  <div class="strip" role="group" aria-label="Drop the tracks on a case">
    <div class="head">Drop {trackDrag.count > 0 ? plural(trackDrag.count, "track") : "tracks"} on a case</div>
    {#if !jobs.canWrite}
      <p class="muted pad">Cases can't be changed right now (a job is running or the library is read-only).</p>
    {:else}
      <div class="list">
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div class="item new" class:over={over === "new"} ondragenter={(e) => allow(e, "new")} ondragover={(e) => allow(e, "new")} ondragleave={() => (over = null)} ondrop={(e) => drop(e, "new")}>
          + New case…
        </div>
        {#each list as c (c.id)}
          <!-- svelte-ignore a11y_no_static_element_interactions -->
          <div
            class="item"
            class:over={over === c.id}
            class:self={c.id === openId}
            ondragenter={(e) => allow(e, c.id)}
            ondragover={(e) => allow(e, c.id)}
            ondragleave={() => (over = null)}
            ondrop={(e) => drop(e, c.id)}
            title={c.id === openId ? "The case that is open" : c.name}
          >
            <span class="name">{c.name}</span>
            <span class="n">{c.track_count}</span>
          </div>
        {/each}
      </div>
    {/if}
  </div>
{/if}

{#if newFrom}
  <NameDialog
    title={`New case from ${plural(newFrom.length, "dragged track")}`}
    confirmLabel="Create"
    taken={list.map((c) => c.name)}
    onconfirm={async (name) => {
      const ids = newFrom ?? [];
      newFrom = null;
      const id = await cases.create(name, ids);
      if (id !== undefined) toast.show(`Created “${name}” with ${plural(ids.length, "track")}`, "ok");
    }}
    onclose={() => (newFrom = null)}
  />
{/if}

<style>
  .strip {
    position: fixed;
    z-index: 25;
    left: 8px;
    bottom: 12px;
    width: 176px;
    max-height: 60vh;
    display: flex;
    flex-direction: column;
    background: var(--panel);
    border: 1px solid var(--accent);
    border-radius: 6px;
    box-shadow: 0 4px 18px color-mix(in srgb, var(--bg) 80%, transparent);
  }
  .head {
    padding: 6px 10px;
    font-size: 11px;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--accent);
    border-bottom: 1px solid var(--line);
  }
  .list {
    overflow: auto;
    padding: 4px;
  }
  .item {
    display: flex;
    justify-content: space-between;
    gap: 6px;
    padding: 5px 8px;
    border-radius: 4px;
    border: 1px dashed transparent;
  }
  .item.over {
    background: var(--accent-soft);
    border-color: var(--accent);
    color: var(--accent);
  }
  .item.self {
    opacity: 0.5;
  }
  .new {
    color: var(--accent);
  }
  .name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .n {
    color: var(--muted);
  }
  .pad {
    margin: 0;
    padding: 8px 10px;
  }
</style>
