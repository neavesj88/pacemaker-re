<script lang="ts">
  // Always-mounted overlay of the tracks area: pick a case (or start a new one) for the tracks
  // the context menu or inspector sent to `addToCase.ask(ids)`.
  import { cases } from "$lib/state/cases.svelte";
  import { session } from "$lib/state/session.svelte";
  import { toast } from "$lib/state/toast.svelte";
  import Button from "$lib/ui/Button.svelte";
  import Modal from "$lib/ui/Modal.svelte";
  import { addToCase } from "./addToCase.svelte";
  import ViewSync from "./ViewSync.svelte";

  const ids = $derived(addToCase.ids);
  const list = $derived(session.library?.cases ?? []);
  let name = $state("");

  $effect(() => {
    if (ids) name = "";
  });

  const noun = (n: number) => `${n} track${n === 1 ? "" : "s"}`;

  async function into(caseId: number, caseName: string) {
    if (!ids) return;
    const n = await cases.addTracks(caseId, ids);
    addToCase.close();
    if (n !== undefined) toast.show(n === 0 ? `Already in “${caseName}”` : `Added ${noun(n)} to “${caseName}”`, "ok");
  }

  async function fresh() {
    const t = name.trim();
    if (!ids || !t) return;
    const id = await cases.create(t, ids);
    addToCase.close();
    if (id !== undefined) toast.show(`Created “${t}” with ${noun(ids.length)}`, "ok");
  }
</script>

<ViewSync />

{#if ids}
  <Modal title={`Add ${noun(ids.length)} to a case`} onclose={() => addToCase.close()}>
    <div class="list">
      {#each list as c (c.id)}
        <button type="button" class="row" onclick={() => into(c.id, c.name)}>
          <span class="n">{c.name}</span>
          <span class="muted">{c.track_count}</span>
        </button>
      {:else}
        <p class="muted">No cases yet. Name one below to create it.</p>
      {/each}
    </div>
    <form
      onsubmit={(e) => {
        e.preventDefault();
        void fresh();
      }}
    >
      <input bind:value={name} maxlength="255" aria-label="New case name" placeholder="New case name" />
    </form>
    {#snippet actions()}
      <Button onclick={() => addToCase.close()}>Cancel</Button>
      <Button variant="primary" disabled={!name.trim()} onclick={fresh}>Create case</Button>
    {/snippet}
  </Modal>
{/if}

<style>
  .list {
    display: flex;
    flex-direction: column;
    max-height: 40vh;
    overflow-y: auto;
    margin-bottom: 10px;
  }
  .row {
    display: flex;
    justify-content: space-between;
    gap: 12px;
    padding: 6px 8px;
    background: transparent;
    border: 0;
    color: inherit;
    text-align: left;
    cursor: pointer;
  }
  .row:hover,
  .row:focus-visible {
    background: var(--hover, rgba(255, 255, 255, 0.08));
  }
  .n {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  input {
    width: 100%;
  }
</style>
