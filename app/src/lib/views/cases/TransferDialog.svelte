<script lang="ts">
  import type { TransferPlan } from "$lib/api/cases";
  import { cases } from "$lib/state/cases.svelte";
  import { session } from "$lib/state/session.svelte";
  import { toast } from "$lib/state/toast.svelte";
  import Button from "$lib/ui/Button.svelte";
  import Modal from "$lib/ui/Modal.svelte";
  import { plural, trackLabel, transferLine } from "./caseLogic";

  // Copy or move tracks from the open case into another case (or a new one). Every track is
  // listed with what will happen to it; the sentences come from `cases_transfer_preview`, which
  // runs the same function the write runs. A move takes the tracks out of this case only: they
  // stay in the library and in every other case.
  let { mode, from, ids, onclose }: { mode: "copy" | "move"; from: { id: number; name: string }; ids: number[]; onclose: () => void } = $props();

  const move = $derived(mode === "move");
  // the device copy of a smart case is rewritten from its rules on every refresh, so it is not offered
  const others = $derived(cases.infos.filter((c) => c.id !== from.id && !cases.smart.some((s) => s.device_case_id === c.id)));
  let choice = $state<number | "new" | null>(null);
  let newName = $state("");
  let plan = $state<TransferPlan | null>(null);
  let working = $state(false);

  const byId = $derived(new Map((session.library?.tracks ?? []).map((t) => [t.track_id, t])));
  const dest = $derived(typeof choice === "number" ? others.find((c) => c.id === choice) : undefined);
  const destName = $derived(choice === "new" ? newName.trim() || "the new case" : (dest?.name ?? ""));
  const nameOk = $derived(newName.trim().length > 0 && [...newName].length <= 255);
  const ready = $derived(!working && ((choice === "new" && nameOk) || (typeof choice === "number" && plan !== null)));

  // a new case starts empty, so everything that is in this case is added; an existing one is
  // asked, so the preview is the plan the write follows
  let seq = 0;
  $effect(() => {
    const to = choice;
    const mine = ++seq;
    plan = null;
    if (typeof to !== "number") return;
    cases.previewTransfer(from.id, to, ids, move).then((p) => {
      if (mine === seq) plan = p ?? null;
    });
  });

  const added = $derived(choice === "new" ? ids.filter((id) => byId.has(id)).length : (plan?.to_add.length ?? 0));
  const removed = $derived(move ? (choice === "new" ? ids.filter((id) => byId.has(id)).length : (plan?.to_remove.length ?? 0)) : 0);

  function line(id: number): string {
    if (choice === "new") return byId.has(id) ? (move ? `will move from “${from.name}” to “${destName}”` : `will be added to “${destName}”`) : "is not in the library: left alone";
    return plan ? transferLine(id, plan, destName, from.name, move) : "";
  }

  async function run() {
    if (!ready) return;
    working = true;
    let to = typeof choice === "number" ? choice : undefined;
    if (choice === "new") to = await cases.create(newName.trim(), []);
    const done = to === undefined ? undefined : await cases.transfer(from.id, to, ids, move);
    working = false;
    if (!done) return;
    toast.show(
      move
        ? `Moved ${plural(done.removed, "track")} from “${from.name}” to “${destName}”${done.added < done.removed ? ` (${done.removed - done.added} already there)` : ""}`
        : `Copied ${plural(done.added, "track")} to “${destName}”${done.added < ids.length ? ` (${ids.length - done.added} already there)` : ""}`,
      "ok",
    );
    onclose();
  }
</script>

<Modal title={`${move ? "Move" : "Copy"} ${plural(ids.length, "track")} ${move ? "to another case" : "to a case"}`} {onclose}>
  <div class="body">
  <label class="field">
    <span class="lbl">{move ? "Move to" : "Copy to"}</span>
    <select bind:value={choice} aria-label="Destination case">
      <option value={null}>Choose a case…</option>
      {#each others as c (c.id)}<option value={c.id}>{c.name} ({plural(c.track_count, "track")})</option>{/each}
      <option value="new">A new case…</option>
    </select>
  </label>
  {#if choice === "new"}
    <input class="text" bind:value={newName} maxlength="255" aria-label="Name of the new case" placeholder="Name of the new case" />
    {#if newName.trim() && others.some((c) => c.name.trim().toLowerCase() === newName.trim().toLowerCase())}
      <p class="warn" role="status">A case with this name already exists. Two cases with the same name are hard to tell apart.</p>
    {/if}
  {/if}

  {#if choice !== null}
    <p class="sum" aria-live="polite">
      {#if choice === "new" || plan}
        {plural(added, "track")} will be added to “{destName}”{move ? `, and ${plural(removed, "track")} taken out of “${from.name}”` : ""}.
        {#if move}They stay in the library and in every other case.{/if}
      {:else}
        Working out what would change…
      {/if}
    </p>
    <ul>
      {#each ids as id (id)}<li><b>{trackLabel(id, byId.get(id))}</b> {line(id)}</li>{/each}
    </ul>
  {/if}
  </div>

  {#snippet actions()}
    <Button data-autofocus onclick={onclose}>Cancel</Button>
    <Button variant="primary" disabled={!ready} onclick={run}>{move ? "Move" : "Copy"} {plural(ids.length, "track")}</Button>
  {/snippet}
</Modal>

<style>
  .body {
    max-height: calc(100vh - 190px);
    overflow: auto;
    padding-right: 2px;
  }
  .field {
    display: grid;
    gap: 2px;
  }
  .lbl {
    font-size: 11px;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--muted);
  }
  select,
  .text {
    width: 100%;
    box-sizing: border-box;
  }
  .text {
    margin-top: 6px;
  }
  .warn {
    margin: 4px 0 0;
    color: var(--bad-text);
  }
  .sum {
    margin: 10px 0 6px;
  }
  ul {
    margin: 0;
    padding: 6px 6px 6px 22px;
    max-height: 220px;
    overflow: auto;
    background: var(--deep);
    border: 1px solid var(--line);
    border-radius: 4px;
  }
  b {
    font-weight: 600;
  }
</style>
