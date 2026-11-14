<script lang="ts">
  import { libraryLabel } from "$lib/state/place";
  import { jobs } from "$lib/state/jobs.svelte";
  import { session } from "$lib/state/session.svelte";
  import Button from "$lib/ui/Button.svelte";
  import Modal from "$lib/ui/Modal.svelte";
  import { acceptedOf, plural, tickKey } from "./cleanupLogic";
  import { cleanup } from "./cleanupState.svelte";
  import DiffText from "./DiffText.svelte";
  import { fieldLabel } from "./tagwriteState.svelte";

  let { onTagsReady }: { onTagsReady: () => void } = $props();

  // What the recipes would change, one line per field per track: old and new side by side, a tick
  // on each line. Apply writes only the ticked lines, after a confirmation that lists every one.
  const PAGE = 400;
  let only = $state<string>(""); // "" = every field
  let shown = $state(PAGE);
  let confirming = $state(false);

  const preview = $derived(cleanup.preview);
  const flat = $derived(
    (preview?.rows ?? []).flatMap((r) =>
      r.changes.map((c, i) => ({
        track_id: r.track_id,
        label: [r.artist, r.title].filter(Boolean).join(" - ") || `Track ${r.track_id}`,
        first: i === 0,
        ...c,
      })),
    ),
  );
  const fieldCounts = $derived.by(() => {
    const m = new Map<string, number>();
    for (const c of flat) m.set(c.field, (m.get(c.field) ?? 0) + 1);
    return [...m];
  });
  const visible = $derived(only ? flat.filter((c) => c.field === only) : flat);
  const accepted = $derived(acceptedOf(preview, cleanup.unticked));
  const labelOf = $derived(new Map(flat.map((c) => [c.track_id, c.label])));
  const t = $derived(cleanup.totals);
  const canWrite = $derived(!!session.library && session.library.problems.length === 0);

  const isOn = (id: number, f: string) => !cleanup.unticked[tickKey(id, f)];
  function setAllShown(on: boolean) {
    for (const c of visible) if (!c.problem) cleanup.setTicked(c.track_id, c.field, on);
  }

  async function apply() {
    confirming = false;
    if (await cleanup.apply()) onTagsReady();
  }
</script>

{#if cleanup.ids.length === 0}
  <p class="empty">No tracks to look at. Choose which tracks above: highlight some in the Tracks view, pick a case, or take the whole library.</p>
{:else if !preview}
  <p class="empty">{cleanup.busy ? "Looking through the tracks…" : "Nothing to show yet."}</p>
{:else if flat.length === 0}
  <p class="empty" role="status">
    Nothing to change: the ticked recipes leave all {plural(preview.scanned, "track")} as they are.
  </p>
{:else}
  <div class="bar">
    <span role="status">
      <strong class="num">{t.fields}</strong> of {plural(flat.length, "change")} ticked, on <strong class="num">{plural(t.tracks, "track")}</strong>
      (of {preview.scanned} looked at){#if t.blocked}; {t.blocked} cannot be applied{/if}
    </span>
    <span class="sp"></span>
    <Button small onclick={() => setAllShown(true)}>Tick all{only ? ` ${fieldLabel(only)}` : ""}</Button>
    <Button small onclick={() => setAllShown(false)}>Untick all{only ? ` ${fieldLabel(only)}` : ""}</Button>
  </div>

  <div class="chips" role="group" aria-label="Show changes to">
    <Button small active={only === ""} onclick={() => { only = ""; shown = PAGE; }}>All fields ({flat.length})</Button>
    {#each fieldCounts as [f, n] (f)}
      <Button small active={only === f} onclick={() => { only = f; shown = PAGE; }}>{fieldLabel(f)} ({n})</Button>
    {/each}
  </div>

  <div class="scroll">
    <table>
      <thead>
        <tr><th class="tick"><span class="sr">Apply</span></th><th>Track</th><th>Field</th><th>Before</th><th>After</th></tr>
      </thead>
      <tbody>
        {#each visible.slice(0, shown) as c (c.track_id + ":" + c.field)}
          <tr class:off={!isOn(c.track_id, c.field)} class:bad={!!c.problem}>
            <td class="tick">
              <input
                type="checkbox"
                checked={isOn(c.track_id, c.field) && !c.problem}
                disabled={!!c.problem}
                aria-label={`Apply the ${fieldLabel(c.field)} change to ${c.label}`}
                onchange={(e) => cleanup.setTicked(c.track_id, c.field, e.currentTarget.checked)}
              />
            </td>
            <td class="trk" class:cont={!c.first && !only}>{c.label}</td>
            <td class="fld">{fieldLabel(c.field)}</td>
            <td><DiffText old={c.old} new={c.new} side="old" /></td>
            <td>
              <DiffText old={c.old} new={c.new} side="new" />
              {#if c.problem}<div class="why">Cannot be applied: {c.problem}.</div>{/if}
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
    {#if visible.length > shown}
      <div class="more">
        Showing {shown} of {visible.length} changes.
        <Button small onclick={() => (shown += PAGE)}>Show {Math.min(PAGE, visible.length - shown)} more</Button>
        <Button small onclick={() => (shown = visible.length)}>Show all</Button>
        <span class="muted">Ticks apply to every change, shown or not.</span>
      </div>
    {/if}
  </div>

  <div class="foot">
    <span class="muted">Applying writes to the library {session.isComputer ? 'on this computer' : `on the device (${libraryLabel('pacemaker', session.library?.root ?? '')})`}; a copy of music.db is kept first. Audio files are not touched.</span>
    <span class="sp"></span>
    <Button
      variant="primary"
      disabled={accepted.length === 0 || cleanup.applying || !canWrite || jobs.busy}
      title={!canWrite ? "This library is read-only here" : jobs.busy ? "Another job is still running" : undefined}
      onclick={() => (confirming = true)}
    >
      {cleanup.applying ? "Applying…" : `Apply ${plural(accepted.length, "change")}…`}
    </Button>
  </div>
{/if}

{#if confirming}
  <Modal title={`Apply ${plural(accepted.length, "change")} to ${plural(t.tracks, "track")}?`} onclose={() => (confirming = false)}>
    <p class="muted">
      These replace the values in the library on {session.library?.root}. A safety copy of music.db is taken first. Nothing else
      changes{cleanup.settings?.write_back ? "; audio files are only touched later, after a second list you confirm" : ": audio files are not touched"}.
    </p>
    <ul class="names">
      {#each accepted as a (a.track_id + ":" + a.field)}
        <li><strong>{labelOf.get(a.track_id) ?? `Track ${a.track_id}`}</strong>, {fieldLabel(a.field)}: <span class="v">“{a.old}”</span> becomes <span class="v">“{a.new}”</span></li>
      {/each}
    </ul>
    {#snippet actions()}
      <Button data-autofocus onclick={() => (confirming = false)}>Cancel</Button>
      <Button variant="primary" onclick={apply}>Apply {accepted.length}</Button>
    {/snippet}
  </Modal>
{/if}

<style>
  .bar,
  .foot {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
    padding: 6px 0;
  }
  .foot {
    font-size: 12px;
    padding-top: 10px;
  }
  .sp {
    flex: 1;
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    padding-bottom: 8px;
  }
  .scroll {
    border: 1px solid var(--line);
    border-radius: 6px;
    overflow: auto;
    max-height: calc(100vh - 380px);
    min-height: 160px;
  }
  table {
    width: 100%;
    border-collapse: collapse;
    table-layout: fixed;
  }
  th {
    position: sticky;
    top: 0;
    background: var(--panel);
    text-align: left;
    font-weight: 600;
    font-size: 12px;
    color: var(--muted);
    padding: 5px 8px;
    border-bottom: 1px solid var(--line);
  }
  td {
    padding: 4px 8px;
    vertical-align: top;
    border-bottom: 1px solid var(--line);
  }
  tbody tr:nth-child(even) {
    background: var(--row-alt);
  }
  th.tick,
  td.tick {
    width: 34px;
    text-align: center;
  }
  th:nth-child(2) {
    width: 24%;
  }
  th:nth-child(3) {
    width: 100px;
  }
  .trk {
    font-weight: 600;
    overflow-wrap: anywhere;
  }
  .trk.cont {
    font-weight: normal;
    color: var(--muted);
  }
  .fld {
    color: var(--muted);
  }
  tr.off td:not(.tick) {
    opacity: 0.45;
  }
  tr.bad td {
    background: var(--bad-bg);
  }
  .why {
    color: var(--bad-text);
    font-size: 12px;
    margin-top: 2px;
  }
  .more {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
    padding: 8px;
  }
  input[type="checkbox"] {
    padding: 0;
    accent-color: var(--accent);
  }
  .sr {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip: rect(0 0 0 0);
  }
  .names {
    margin: 8px 0 0;
    padding-left: 18px;
    max-height: 300px;
    overflow: auto;
    overflow-wrap: anywhere;
  }
  .names li {
    content-visibility: auto;
    contain-intrinsic-size: auto 1.4em;
  }
  .v {
    white-space: pre-wrap;
    color: var(--text);
  }
</style>
