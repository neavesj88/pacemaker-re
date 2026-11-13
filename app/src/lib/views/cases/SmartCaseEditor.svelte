<script lang="ts">
  import { evaluateRules, type Preview, type Query, type SmartCase, type SortField } from "$lib/api/cases";
  import { energy } from "$lib/state/energy.svelte";
  import { energySource } from "$lib/state/energySource.svelte";
  import { labels } from "$lib/state/labels.svelte";
  import { caseExport } from "$lib/state/caseExport.svelte";
  import { cases } from "$lib/state/cases.svelte";
  import { jobs } from "$lib/state/jobs.svelte";
  import { session } from "$lib/state/session.svelte";
  import { smartSettings } from "$lib/state/smartSettings.svelte";
  import { MAX_LIMIT, SORT_FIELDS, describeQuery, directionLabels, newQuery, stampCaseNames, validateQuery } from "$lib/state/smartRules";
  import { toast } from "$lib/state/toast.svelte";
  import Button from "$lib/ui/Button.svelte";
  import ContextMenu, { type MenuItem } from "$lib/ui/ContextMenu.svelte";
  import ConfirmDialog from "./ConfirmDialog.svelte";
  import { exportMenuItems, PLAIN_LABELS } from "./exportMenu";
  import SmartPreview from "./SmartPreview.svelte";
  import SmartRuleGroup from "./SmartRuleGroup.svelte";

  // The rule builder and the matching tracks of one smart case (or a new one, id 0). Rules are
  // saved in the app's config dir; nothing reaches the device until "Put on Pacemaker" is switched on.
  const id = cases.smartOpen ?? 0;

  const stored = (): SmartCase | undefined => cases.smart.find((s) => s.id === id);
  const start: SmartCase = stored()
    ? JSON.parse(JSON.stringify(stored()))
    : { id: 0, name: "New smart case", ...newQuery(), on_device: false, device_case_id: null };

  const asQuery = (d: SmartCase): Query => ({ mode: d.mode, rules: d.rules, sort: d.sort, limit: d.limit });
  const stateOf = (d: SmartCase) => JSON.stringify({ name: d.name, ...asQuery(d) });

  let draft = $state<SmartCase>(start);
  let baseline = $state(stateOf(start));
  let dirty = $derived(stateOf(draft) !== baseline);
  let sc = $derived(cases.smart.find((s) => s.id === id));

  // ---- order and limit --------------------------------------------------------------------
  // The limit box can be empty while typing, which the rule set would read as "no limit": keep
  // the on/off switch and the number apart and write NaN (refused with a message) for an empty box.
  let limitOn = $state(start.limit != null);
  let limitN = $state<number | null>(start.limit ?? 50);
  $effect(() => {
    draft.limit = limitOn ? (limitN ?? Number.NaN) : null;
  });

  function pickSort(field: string) {
    if (!field) draft.sort = null;
    else {
      const f = field as SortField;
      // dates read best newest first; everything else starts ascending
      draft.sort = { field: f, descending: f === "date_added" || f === "last_played" };
    }
  }
  let dirs = $derived(draft.sort ? directionLabels(draft.sort.field) : null);
  let sortLabel = $derived(SORT_FIELDS.find((f) => f.field === draft.sort?.field)?.label ?? "");

  // ---- live matches -----------------------------------------------------------------------
  let preview = $state<Preview>({ ids: [], matched: 0, values: [] });
  let ruleError = $state("");
  let counting = $state(false);
  let ticket = 0;

  // Mistakes the backend would refuse, said next to the rules (a cleared number box gives null).
  let fieldError = $derived(validateQuery(asQuery(draft)));
  const caseName = (cid: number) => cases.infos.find((c) => c.id === cid)?.name ?? `missing case ${cid}`;
  let summary = $derived(fieldError ? "" : describeQuery(asQuery(draft), caseName));

  $effect(() => {
    const query = $state.snapshot(asQuery(draft)) as Query;
    const root = session.library?.root;
    // library tracks are read so a reload (a refresh after a write) re-runs the count
    void session.library?.tracks;
    // tag, colour and energy rules read app-side labels and the Energy source setting
    void labels.version;
    void energySource.source;
    void energy.state;
    if (!root) return;
    if (fieldError) {
      ticket++;
      preview = { ids: [], matched: 0, values: [] };
      counting = false;
      return;
    }
    const mine = ++ticket;
    counting = true;
    const timer = setTimeout(async () => {
      try {
        const out = await evaluateRules(root, query);
        if (mine === ticket) (preview = out, (ruleError = ""));
      } catch (e) {
        if (mine === ticket) (preview = { ids: [], matched: 0, values: [] }, (ruleError = String(e)));
      } finally {
        if (mine === ticket) counting = false;
      }
    }, 150);
    return () => clearTimeout(timer);
  });

  // ---- actions ----------------------------------------------------------------------------
  let deleting = $state(false);
  let leaving = $state<(() => void) | null>(null);

  // Unsaved edits are not thrown away silently: leaving through the store asks first.
  $effect(() => {
    if (!dirty) return;
    cases.leaveGuard = (proceed) => (leaving = proceed);
    return () => {
      cases.leaveGuard = null;
    };
  });
  let alsoDevice = $state(false);
  let takingOff = $state(false);
  let menu = $state<{ x: number; y: number; items: MenuItem[] } | null>(null);
  $effect(() => {
    void smartSettings.ensure(); // the note under "Put on Pacemaker" names the refresh setting
  });

  // The export is of the saved smart case as it matches now (rekordbox XML cannot hold rules, so
  // it is an ordinary playlist of those tracks), so it waits for unsaved edits to be saved.
  function exportMenu(e: MouseEvent) {
    e.stopPropagation();
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const target = { id, name: draft.name };
    menu = {
      x: r.left,
      y: r.bottom + 2,
      items: exportMenuItems({
        run: (f) => cases.exportSmart(target, f),
        options: () => caseExport.show({ caseIds: [], smartIds: [id] }),
        labels: { ...PLAIN_LABELS, rekordbox: "rekordbox XML (as a playlist)…" },
      }),
    };
  }

  const deviceName = (name: string) => `★ ${name.trim()}`;

  async function save() {
    // case rules remember the case's name, so a rule file read against another card can say so
    stampCaseNames(draft.rules, cases.infos);
    const out = await cases.saveSmart($state.snapshot(draft) as SmartCase);
    if (!out) return;
    baseline = stateOf(out);
    toast.show(`Saved “${out.name}”`, "ok");
    if (id === 0) cases.openSmart(out.id);
  }
</script>

<div class="crumb">
  <Button small onclick={() => cases.leave(() => cases.close())}>← Cases</Button>
  <span class="muted">Smart case</span>
  <input class="name" bind:value={draft.name} aria-label="Smart case name" maxlength="200" />
  <Button variant="primary" onclick={save} disabled={!dirty || draft.name.trim() === "" || !!ruleError || !!fieldError}>
    {id === 0 ? "Create" : "Save"}
  </Button>
  {#if id !== 0}
    <Button
      onclick={exportMenu}
      disabled={dirty || !!ruleError || !!fieldError || preview.ids.length === 0}
      title={dirty ? "Save your changes first: the export uses the saved rules" : "Save the tracks this smart case matches right now as a list for other software"}
    >Export…</Button>
    <Button onclick={() => ((alsoDevice = false), (deleting = true))}>Delete…</Button>
  {/if}
</div>

<div class="builder">
  <div class="rules">
    <SmartRuleGroup bind:mode={draft.mode} bind:items={draft.rules} />

    <div class="order">
      <label>
        <span class="muted">Order by</span>
        <select value={draft.sort?.field ?? ""} onchange={(e) => pickSort(e.currentTarget.value)} aria-label="Order by">
          <option value="">Artist, then title (default)</option>
          {#each SORT_FIELDS as f (f.field)}<option value={f.field}>{f.label}</option>{/each}
        </select>
      </label>
      {#if draft.sort && dirs}
        <select bind:value={draft.sort.descending} aria-label="Direction">
          <option value={false}>{dirs[0]}</option>
          <option value={true}>{dirs[1]}</option>
        </select>
        <span class="muted">Tracks without a {sortLabel.toLowerCase()} come last.</span>
      {/if}
    </div>
    <div class="order">
      <label class="check"><input type="checkbox" bind:checked={limitOn} /> Only the first</label>
      <input type="number" min="1" max={MAX_LIMIT} step="1" bind:value={limitN} disabled={!limitOn} aria-label="How many tracks" />
      <span class="muted">tracks, after ordering{limitOn ? "" : " (off: every match)"}</span>
    </div>

    {#if summary}<p class="summary muted">{summary}</p>{/if}
  </div>

  <aside class="side">
    <div class="count" aria-live="polite">
      {#if fieldError}
        <span class="err">{fieldError}</span>
      {:else if ruleError}
        <span class="err">{ruleError}</span>
      {:else}
        <b>{preview.ids.length}</b> track{preview.ids.length === 1 ? "" : "s"} match{counting ? "…" : ""}
        {#if preview.matched > preview.ids.length}
          <div class="muted cut">the first {preview.ids.length} of {preview.matched} that matched</div>
        {/if}
      {/if}
    </div>

    <div class="device">
      <div class="row">
        <button
          class="switch"
          role="switch"
          aria-checked={sc?.on_device ?? false}
          aria-label="Put on Pacemaker"
          disabled={id === 0 || dirty || !!ruleError || !!fieldError || !jobs.canWrite}
          onclick={() => (sc?.on_device ? (takingOff = true) : cases.setSmartOnDevice(id, true))}
        ><span></span></button>
        <b>Put on Pacemaker</b>
      </div>
      <p class="muted">
        {#if id === 0}
          Save the smart case first.
        {:else if dirty}
          Save your changes first: the Pacemaker copy is made from the saved rules.
        {:else if sc?.on_device}
          The Pacemaker has a case called “{deviceName(sc.name)}” with these tracks{draft.sort ? ", in this order" : ""}.
          {#if smartSettings.value.refresh_mode === "auto"}
            It is refreshed automatically when the library changes (Cases › Smart case settings).
          {:else if smartSettings.value.refresh_mode === "ask"}
            When the library changes you are asked whether to refresh it (Cases › Smart case settings).
          {:else}
            It does not change by itself: refresh it after the library changes.
          {/if}
        {:else}
          Off: this smart case exists only in this app. Switch it on to write a normal case called “{deviceName(draft.name)}” to the Pacemaker.
        {/if}
      </p>
      {#if sc?.on_device && !dirty}
        <Button small disabled={!jobs.canWrite || !!ruleError} onclick={() => cases.setSmartOnDevice(id, true)}>Refresh on Pacemaker now</Button>
      {/if}
    </div>
  </aside>
</div>

<SmartPreview ids={preview.ids} values={preview.values} valueLabel={sortLabel} />
{#if preview.ids.length === 0 && !ruleError && !fieldError}
  <!-- a slow card can take seconds to answer; "no matches" before it has would be a wrong answer -->
  <p class="empty">{counting ? "Working out which tracks match…" : "No tracks match these rules."}</p>
{/if}

{#if menu}
  <ContextMenu x={menu.x} y={menu.y} items={menu.items} onclose={() => (menu = null)} />
{/if}

{#if leaving}
  {@const proceed = leaving}
  <ConfirmDialog
    title="Discard unsaved changes?"
    message={`The smart case “${draft.name.trim() || "New smart case"}” has changes that are not saved. Leaving now discards them.`}
    confirmLabel="Discard changes"
    onconfirm={() => proceed()}
    onclose={() => (leaving = null)}
  />
{/if}

{#if takingOff && sc}
  <ConfirmDialog
    title="Remove from the Pacemaker?"
    message={`Delete the case “${deviceName(sc.name)}” from the Pacemaker? The smart case stays in this app and no track is deleted; you can put it back at any time.`}
    confirmLabel="Delete device case"
    onconfirm={() => cases.setSmartOnDevice(id, false)}
    onclose={() => (takingOff = false)}
  />
{/if}

{#if deleting && sc}
  <ConfirmDialog
    title="Delete this smart case?"
    message={`Delete the smart case “${sc.name}” (${cases.smartCounts[sc.id] ?? preview.ids.length} matching tracks) from this app? No track is deleted.`}
    confirmLabel="Delete smart case"
    onconfirm={() => cases.deleteSmart(id, alsoDevice)}
    onclose={() => (deleting = false)}
  >
    {#snippet extra()}
      {#if sc.on_device}
        <label class="also">
          <input type="checkbox" bind:checked={alsoDevice} />
          Also delete the case “{deviceName(sc.name)}” from the Pacemaker (otherwise it stays there as an ordinary case)
        </label>
      {/if}
    {/snippet}
  </ConfirmDialog>
{/if}

<style>
  .crumb {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 10px;
    border-bottom: 1px solid var(--line);
    flex-wrap: wrap;
  }
  .name {
    min-width: 220px;
    font-weight: 600;
  }
  .builder {
    display: flex;
    gap: 16px;
    padding: 10px;
    border-bottom: 1px solid var(--line);
    flex-wrap: wrap;
  }
  .rules {
    flex: 1;
    min-width: 360px;
    display: grid;
    gap: 10px;
    align-content: start;
  }
  .order {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }
  .order label {
    display: inline-flex;
    align-items: center;
    gap: 8px;
  }
  .order input[type="number"] {
    width: 90px;
  }
  .check {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }
  .summary {
    margin: 0;
    max-width: 70ch;
  }
  .cut {
    font-size: 12px;
    margin-top: 2px;
  }
  .side {
    width: 300px;
    display: grid;
    gap: 12px;
    align-content: start;
  }
  .count {
    font-size: 14px;
  }
  .count b {
    font-size: 20px;
    color: var(--green);
  }
  .err {
    color: var(--bad);
  }
  .device {
    padding: 10px;
    background: var(--panel);
    border: 1px solid var(--line);
    border-radius: 6px;
  }
  .device p {
    margin: 6px 0;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .switch {
    width: 34px;
    height: 18px;
    padding: 2px;
    border-radius: 9px;
    border: 1px solid var(--line-hi);
    background: var(--deep);
    cursor: pointer;
    display: flex;
    align-items: center;
  }
  .switch span {
    width: 12px;
    height: 12px;
    border-radius: 50%;
    background: var(--muted);
    transition: transform 0.12s;
  }
  .switch[aria-checked="true"] {
    background: var(--accent-soft);
    border-color: var(--accent);
  }
  .switch[aria-checked="true"] span {
    background: var(--accent);
    transform: translateX(16px);
  }
  .switch:disabled {
    opacity: 0.45;
    cursor: default;
  }
  .also {
    display: flex;
    gap: 8px;
    align-items: flex-start;
    margin-bottom: 8px;
  }
  .also input {
    margin-top: 3px;
  }
</style>
