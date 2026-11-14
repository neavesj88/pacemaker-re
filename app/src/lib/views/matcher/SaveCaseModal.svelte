<script module lang="ts">
  export type Entry = { line: number; requested: string; track: string; note: string };
</script>

<script lang="ts">
  import { untrack } from "svelte";
  import { libraryKind, libraryLabel } from "$lib/state/place";
  import { session } from "$lib/state/session.svelte";
  import Button from "$lib/ui/Button.svelte";
  import Modal from "$lib/ui/Modal.svelte";

  // Confirms the one write the matcher makes: a new case holding the owned tracks. Lists every
  // track that goes in (the request line and the library track it stands for), so nothing
  // is added that the user has not seen.
  let {
    root,
    entries,
    trackCount,
    notOwned,
    undecided,
    initial,
    taken,
    onconfirm,
    onclose,
  }: {
    root: string;
    entries: Entry[];
    /** Distinct tracks the case will hold (a track asked for twice counts once). */
    trackCount: number;
    notOwned: number;
    undecided: number;
    initial: string;
    /** Existing case names, to warn about a clash. */
    taken: string[];
    onconfirm: (name: string) => void;
    onclose: () => void;
  } = $props();

  // named the way the header names it ("This computer", "Pacemaker (G:)"), never by its folder
  const place = $derived.by(() => {
    const kind = libraryKind(root);
    return { kind, label: libraryLabel(kind, root, session.computer?.path ?? null) };
  });
  let name = $state(untrack(() => initial));
  const valid = $derived(name.trim().length > 0 && [...name].length <= 255);
  const clash = $derived(taken.some((t) => t.trim().toLowerCase() === name.trim().toLowerCase()) && name.trim() !== "");
  const repeats = $derived(entries.length - trackCount);

  function submit() {
    if (valid) onconfirm(name.trim());
  }
</script>

<Modal title="Save owned tracks as a case" {onclose}>
  <p>
    Creates a new case {place.kind === "computer" ? "in" : "on"} <b>{place.label}</b> holding <b>{trackCount}</b> track{trackCount === 1 ? "" : "s"}, in the order of the request list.
    This is a write to the library; no audio is added, changed or deleted.
  </p>
  <form
    onsubmit={(e) => {
      e.preventDefault();
      submit();
    }}
  >
    <input bind:value={name} maxlength="255" aria-label="Case name" placeholder="Case name" />
  </form>
  {#if clash}<p class="warn" role="status">A case with this name already exists. Two cases with the same name are hard to tell apart.</p>{/if}
  <ol>
    {#each entries as e (e.line)}
      <li>
        <span class="muted">line {e.line}:</span> {e.requested}
        <span class="arrow">→</span> <b>{e.track}</b>
        {#if e.note}<div class="muted sub">{e.note}</div>{/if}
      </li>
    {/each}
  </ol>
  <p class="muted">
    {#if repeats > 0}{repeats} line{repeats === 1 ? "" : "s"} point{repeats === 1 ? "s" : ""} at a track already listed and {repeats === 1 ? "is" : "are"} counted once.{/if}
    {notOwned} line{notOwned === 1 ? "" : "s"} not owned {notOwned === 1 ? "is" : "are"} left out.
    {#if undecided > 0}{undecided} line{undecided === 1 ? "" : "s"} still waiting for your decision {undecided === 1 ? "is" : "are"} left out too.{/if}
  </p>
  {#snippet actions()}
    <Button data-autofocus onclick={onclose}>Cancel</Button>
    <Button variant="primary" disabled={!valid} onclick={submit}>Save case</Button>
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
  ol {
    margin: 10px 0 0;
    padding: 0 0 0 4px;
    list-style: none;
    max-height: 260px;
    overflow-y: auto;
    border: 1px solid var(--line);
    border-radius: 4px;
  }
  li {
    padding: 4px 10px;
    font-size: 13px;
    overflow-wrap: anywhere;
  }
  li + li {
    border-top: 1px solid var(--line);
  }
  .sub {
    font-size: 12px;
  }
  .arrow {
    color: var(--accent);
  }
</style>
