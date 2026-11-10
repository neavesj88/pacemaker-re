<script lang="ts">
  import Button from "$lib/ui/Button.svelte";
  import Modal from "$lib/ui/Modal.svelte";

  // Names the case that "Save as case" will create, and says exactly what will be written.
  let {
    initialName,
    count,
    source,
    busy = false,
    onsave,
    oncancel,
  }: {
    initialName: string;
    count: number;
    /** Where the tracks come from, worded for the sentence ("the session on Wed 8 Nov"). */
    source: string;
    busy?: boolean;
    onsave: (name: string) => void;
    oncancel: () => void;
  } = $props();

  let name = $state("");
  $effect.pre(() => {
    name = initialName;
  });
  let ok = $derived(name.trim().length > 0 && name.length <= 255 && count > 0);
</script>

<Modal title="Save as case" onclose={oncancel}>
  <p>
    This adds one new case to the device library with the {count} track{count === 1 ? "" : "s"} from {source}, in the order shown.
    No track, existing case or mix is changed.
  </p>
  <label class="f">
    Case name
    <input
      data-autofocus
      bind:value={name}
      maxlength="255"
      onkeydown={(e) => e.key === "Enter" && ok && !busy && onsave(name)}
    />
  </label>
  {#snippet actions()}
    <Button onclick={oncancel}>Cancel</Button>
    <Button variant="primary" disabled={!ok || busy} onclick={() => onsave(name)}>{busy ? "Saving" : `Create case with ${count}`}</Button>
  {/snippet}
</Modal>

<style>
  p {
    margin: 0 0 12px;
    color: var(--muted);
  }
  .f {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  input {
    background: var(--deep);
    color: var(--text);
    border: 1px solid var(--line-hi);
    border-radius: 4px;
    padding: 6px 8px;
    font: inherit;
  }
</style>
