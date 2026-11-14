<script lang="ts">
  import { untrack } from "svelte";
  import { cases } from "$lib/state/cases.svelte";
  import { session } from "$lib/state/session.svelte";
  import { toast } from "$lib/state/toast.svelte";
  import Button from "$lib/ui/Button.svelte";
  import Modal from "$lib/ui/Modal.svelte";
  import { setPrefs } from "./prefs.svelte";
  import { setBuilder } from "./setbuilder.svelte";
  import { clock, solverOptions } from "./solver";

  // Saves the set as a NEW case whose order is the play order. The case the tracks came from is
  // never changed.
  let { onclose }: { onclose: () => void } = $props();

  let ids = $derived(setBuilder.order.filter((id) => setBuilder.byId.has(id)));
  let secs = $derived(setBuilder.summary.secs);
  let taken = $derived((session.library?.cases ?? []).map((c) => c.name));

  function freeName(base: string): string {
    const t = new Set(taken.map((x) => x.trim().toLowerCase()));
    if (!t.has(base.toLowerCase())) return base;
    for (let n = 2; ; n++) if (!t.has(`${base} ${n}`.toLowerCase())) return `${base} ${n}`;
  }

  let name = $state(untrack(() => freeName(setBuilder.source?.kind === "case" ? `${setBuilder.source.name} set` : "Selected tracks set")));
  let busy = $state(false);
  let valid = $derived(name.trim().length > 0 && [...name].length <= 255);
  let clash = $derived(taken.some((t) => t.trim().toLowerCase() === name.trim().toLowerCase()));
  let capNote = $derived(solverOptions(setPrefs.value).maxTrackSecs != null);

  async function save() {
    if (!valid || busy) return;
    busy = true;
    const id = await cases.create(name.trim(), ids);
    busy = false;
    if (id !== undefined) {
      toast.show(`Saved the case “${name.trim()}”: ${ids.length} track${ids.length === 1 ? "" : "s"} in set order`, "ok", 6000);
      onclose();
    }
  }
</script>

<Modal title="Save the set as a case" {onclose}>
  <form
    onsubmit={(e) => {
      e.preventDefault();
      void save();
    }}
  >
    <!-- svelte-ignore a11y_autofocus -->
    <input data-autofocus bind:value={name} maxlength="255" aria-label="Case name" placeholder="Case name" />
  </form>
  <p>
    A new case with {ids.length} track{ids.length === 1 ? "" : "s"} ({clock(secs)}), in the order shown. It is written to the library like any case.
    {#if setBuilder.source?.kind === "case"}The case “{setBuilder.source.name}” is not changed.{/if}
  </p>
  {#if setBuilder.out.length > 0}
    <p class="muted">{setBuilder.out.length} left-out track{setBuilder.out.length === 1 ? " is" : "s are"} not included.</p>
  {/if}
  {#if capNote}
    <p class="muted">A case holds whole tracks: the “longest a track plays” limit only applies when a mix is made.</p>
  {/if}
  {#if clash}
    <p class="warn" role="status">A case with this name already exists. Two cases with the same name are hard to tell apart.</p>
  {/if}
  {#snippet actions()}
    <Button onclick={onclose}>Cancel</Button>
    <Button variant="primary" disabled={!valid || busy || !ids.length} onclick={save}>Save case</Button>
  {/snippet}
</Modal>

<style>
  input {
    width: 100%;
    box-sizing: border-box;
  }
  p {
    margin: 8px 0 0;
  }
  .warn {
    color: var(--bad-text);
  }
</style>
