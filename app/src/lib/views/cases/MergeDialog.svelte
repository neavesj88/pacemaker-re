<script lang="ts">
  import { untrack } from "svelte";
  import type { MergePlan } from "$lib/api/cases";
  import { cases } from "$lib/state/cases.svelte";
  import { session } from "$lib/state/session.svelte";
  import { smartSettings } from "$lib/state/smartSettings.svelte";
  import { toast } from "$lib/state/toast.svelte";
  import Button from "$lib/ui/Button.svelte";
  import Modal from "$lib/ui/Modal.svelte";
  import { caseView } from "./caseView.svelte";
  import { mergeSummary, plural, trackLabel } from "./caseLogic";
  import { remakeWhen } from "./smartRefreshLogic";

  // Merges cases into a new case or an existing one, without repeats. The sentence and the track
  // list come from `cases_merge_preview`, which runs the same function the merge runs, so what is
  // shown here is what gets written. The source cases are only deleted if the box is ticked
  // (off by default; the choice is remembered).
  let { initial, onclose }: { initial: number[]; onclose: () => void } = $props();

  const list = $derived(cases.infos);
  // not offered as a destination: a smart case's device copy is rewritten from its rules on refresh
  const smartOf = (id: number) => cases.smart.find((s) => s.device_case_id === id);
  const destinations = $derived(list.filter((c) => !cases.smart.some((s) => s.device_case_id === c.id)));
  let sources = $state<Set<number>>(new Set(untrack(() => initial)));
  let mode = $state<"new" | "existing">("new");
  let newName = $state("");
  let target = $state<number | null>(null);
  let deleteSources = $state(untrack(() => caseView.prefs.mergeDeletesSources));
  let plan = $state<MergePlan | null>(null);
  let working = $state(false);

  const byId = $derived(new Map((session.library?.tracks ?? []).map((t) => [t.track_id, t])));
  const picked = $derived(list.filter((c) => sources.has(c.id)));
  // smart cases with a rule about one of the cases that would be deleted: they report an error until fixed
  const smartUsing = $derived(cases.smartCasesUsing(picked.map((c) => c.id)));
  const targetCase = $derived(list.find((c) => c.id === target));
  const intoItself = $derived(mode === "existing" && target !== null && sources.has(target));
  const nameOk = $derived(newName.trim().length > 0 && [...newName].length <= 255);
  const destOk = $derived(mode === "new" ? nameOk : target !== null && !intoItself);
  const destName = $derived(mode === "new" ? newName.trim() || "the new case" : (targetCase?.name ?? ""));
  const ready = $derived(picked.length > 0 && destOk && plan !== null && !working);
  const nameTaken = $derived(mode === "new" && list.some((c) => c.name.trim().toLowerCase() === newName.trim().toLowerCase()) && newName.trim() !== "");

  // The preview follows the choices. A counter drops answers that arrive after a newer request.
  let seq = 0;
  $effect(() => {
    const ids = [...sources];
    const dest = mode === "existing" ? target : null;
    const skip = ids.length === 0 || (mode === "existing" && (dest === null || ids.includes(dest)));
    const mine = ++seq;
    if (skip) {
      plan = null;
      return;
    }
    cases.previewMerge(ids, dest).then((p) => {
      if (mine === seq) plan = p ?? null;
    });
  });

  function toggle(id: number) {
    const next = new Set(sources);
    if (!next.delete(id)) next.add(id);
    sources = next;
  }

  async function run() {
    if (!ready || !plan) return;
    working = true;
    caseView.setMergeDeletes(deleteSources);
    const done = await cases.merge([...sources], mode === "new" ? { newName: newName.trim() } : { existing: target! }, deleteSources);
    working = false;
    if (!done) return;
    toast.show(
      `Merged ${plural(picked.length, "case")} into “${destName}”: ${plural(done.added, "track")} added${done.deleted_sources > 0 ? `, ${plural(done.deleted_sources, "source case")} deleted` : ""}`,
      "ok",
    );
    onclose();
  }
</script>

<Modal title="Merge cases" {onclose}>
  <div class="body">
  <div class="cols">
    <fieldset>
      <legend>Cases to merge</legend>
      <div class="box">
        {#each list as c (c.id)}
          <label>
            <input type="checkbox" checked={sources.has(c.id)} onchange={() => toggle(c.id)} />
            <span class="name">{c.name}</span>
            <span class="muted">{plural(c.track_count, "track")}</span>
          </label>
        {/each}
      </div>
    </fieldset>

    <fieldset>
      <legend>Merge into</legend>
      <label class="row">
        <input type="radio" name="dest" value="new" bind:group={mode} />
        <span>A new case</span>
      </label>
      {#if mode === "new"}
        <input class="text" bind:value={newName} maxlength="255" aria-label="Name of the new case" placeholder="Name of the new case" />
        {#if nameTaken}<p class="warn" role="status">A case with this name already exists. Two cases with the same name are hard to tell apart.</p>{/if}
      {/if}
      <label class="row">
        <input type="radio" name="dest" value="existing" bind:group={mode} />
        <span>An existing case (it keeps its order; new tracks go at the end)</span>
      </label>
      {#if mode === "existing"}
        <select bind:value={target} aria-label="Existing case">
          <option value={null}>Choose a case…</option>
          {#each destinations as c (c.id)}<option value={c.id}>{c.name} ({plural(c.track_count, "track")})</option>{/each}
        </select>
        {#if intoItself}<p class="warn" role="status">A case cannot be merged into itself: untick it on the left or pick another case.</p>{/if}
      {/if}
    </fieldset>
  </div>

  <label class="row del">
    <input type="checkbox" bind:checked={deleteSources} />
    <span>Delete the merged cases afterwards (the tracks stay in the library)</span>
  </label>
  {#if deleteSources && picked.length > 0}
    <div class="danger" role="status">
      These cases will be deleted{session.isComputer ? '' : ' from the Pacemaker'}:
      <ul>
        {#each picked as c (c.id)}
          <li>
            “{c.name}” ({plural(c.track_count, "track")}){c.name === "Favourites" ? " – the case the heart on a track uses" : ""}{smartOf(c.id) ? ` – the Pacemaker copy of the smart case “${smartOf(c.id)?.name}”, which will be made again ${remakeWhen(smartSettings.value.refresh_mode)}` : ""}
          </li>
        {/each}
      </ul>
      {#if smartUsing.length > 0}
        <p class="note">
          {smartUsing.map((n) => `“${n}”`).join(", ")} {smartUsing.length === 1 ? "is a smart case that has" : "are smart cases that have"} a rule about {picked.length === 1 ? "this case" : "one of these cases"}; {smartUsing.length === 1 ? "it" : "they"} will report an error until you pick another case in the rule.
        </p>
      {/if}
    </div>
  {/if}

  <div class="preview" aria-live="polite">
    {#if plan}
      <p>{mergeSummary(plan, destName)}</p>
      {#if plan.added.length > 0}
        <ul class="tracks" aria-label="Tracks that will be added">
          {#each plan.added as id (id)}<li>{trackLabel(id, byId.get(id))}</li>{/each}
        </ul>
      {/if}
    {:else if picked.length === 0}
      <p class="muted">Tick the cases to merge.</p>
    {:else}
      <p class="muted">{mode === "existing" && target === null ? "Choose the case to merge into." : intoItself ? "" : "Working out what would be added…"}</p>
    {/if}
  </div>
  </div>

  {#snippet actions()}
    <Button data-autofocus onclick={onclose}>Cancel</Button>
    <Button variant="primary" disabled={!ready} onclick={run}>
      Merge{picked.length > 0 ? ` ${plural(picked.length, "case")}` : ""}{plan ? ` (+${plural(plan.added.length, "track")})` : ""}
    </Button>
  {/snippet}
</Modal>

<style>
  /* the shared Modal does not scroll, so a tall dialog scrolls here on a short window */
  .body {
    max-height: calc(100vh - 190px);
    overflow: auto;
    padding-right: 2px;
  }
  .cols {
    display: grid;
    gap: 10px;
  }
  fieldset {
    margin: 0;
    padding: 6px 10px 8px;
    border: 1px solid var(--line);
    border-radius: 4px;
    min-width: 0;
  }
  legend {
    padding: 0 4px;
    font-size: 11px;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--muted);
  }
  .box {
    max-height: 150px;
    overflow: auto;
  }
  .box label {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 2px 4px;
    border-radius: 3px;
    cursor: pointer;
  }
  .box label:hover {
    background: var(--hover);
  }
  .name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-top: 4px;
  }
  .row input {
    padding: 0;
  }
  .text,
  select {
    width: 100%;
    box-sizing: border-box;
    margin-top: 4px;
  }
  .del {
    margin-top: 10px;
  }
  .warn {
    margin: 4px 0 0;
    color: var(--bad-text);
  }
  .danger {
    margin-top: 6px;
    padding: 6px 10px;
    border-radius: 4px;
    background: var(--bad-bg);
    color: var(--bad-text);
  }
  .danger ul {
    margin: 4px 0 0;
    padding-left: 18px;
  }
  .danger .note {
    margin: 6px 0 0;
  }
  .preview {
    margin-top: 10px;
  }
  .preview p {
    margin: 0 0 6px;
  }
  .tracks {
    margin: 0;
    padding: 6px 6px 6px 22px;
    max-height: 160px;
    overflow: auto;
    background: var(--deep);
    border: 1px solid var(--line);
    border-radius: 4px;
  }
</style>
