<script lang="ts">
  import { session } from "$lib/state/session.svelte";
  import Button from "$lib/ui/Button.svelte";
  import WideModal from "../device/WideModal.svelte";
  import { libs } from "./libraries.svelte";
  import { countText, defaultCasePick, libraryNameFor, NEW_CASE, resolveIntoCase } from "./librariesLogic";

  // The click route into a case: "Copy into a case on <other library>…" asks which case (one of the
  // other library's, or a new one), then plans the copy exactly as dropping the tracks on that case
  // row does and shows the same dialog (CopyDialog). Nothing is written from here.
  const c = $derived(libs.chooser);
  const cases = $derived(c ? (libs.data(c.target)?.cases ?? []) : []);
  const targetName = $derived(c ? libraryNameFor(libs.data(c.target)?.root ?? "", session.computer?.path ?? null) : "");

  let pick = $state(NEW_CASE);
  let typed = $state("");
  let problem = $state("");
  $effect(() => {
    // a fresh question starts on the first case (or a new one when the library has none)
    if (c) {
      pick = defaultCasePick(cases);
      typed = "";
      problem = "";
    }
  });

  function go() {
    if (!c) return;
    const r = resolveIntoCase(cases, pick, typed);
    if ("error" in r) {
      problem = r.error;
      return;
    }
    libs.chooseCaseDone(r.into);
  }
</script>

{#if c}
  <WideModal title={`Copy into a case on ${targetName}`} onclose={() => libs.closeChooser()} width={480}>
    <p class="sum">Copy {countText("tracks", c.ids.length)} to {targetName} and put them in a case there.</p>
    <label class="field">
      <span>Case</span>
      <select bind:value={pick} data-autofocus aria-label={`Case on ${targetName}`} onchange={() => (problem = "")}>
        {#each cases as k (k.id)}<option value={k.name}>{k.name} ({countText("tracks", k.track_count)})</option>{/each}
        <option value={NEW_CASE}>A new case…</option>
      </select>
    </label>
    {#if pick === NEW_CASE}
      <label class="field">
        <span>Name</span>
        <input
          type="text"
          bind:value={typed}
          placeholder="Name of the new case"
          aria-label="Name of the new case"
          oninput={() => (problem = "")}
          onkeydown={(e) => e.key === "Enter" && go()}
        />
      </label>
    {/if}
    {#if problem}<p class="warn" role="alert">{problem}</p>{/if}
    <p class="muted note">Next you see the same list as after a drop: every track, what is done to it, and the room needed. Nothing is written until you press Copy there.</p>

    {#snippet actions()}
      <Button onclick={() => libs.closeChooser()}>Cancel</Button>
      <Button variant="primary" disabled={libs.busy} onclick={go}>Check what would be copied</Button>
    {/snippet}
  </WideModal>
{/if}

<style>
  p {
    margin: 0 0 8px;
  }
  .field {
    display: flex;
    align-items: center;
    gap: 10px;
    margin: 8px 0;
  }
  .field span {
    flex: none;
    width: 48px;
    color: var(--muted);
    font-size: 12px;
  }
  select,
  input {
    flex: 1;
    min-width: 0;
    background: var(--bg);
    color: var(--text);
    border: 1px solid var(--line-hi);
    border-radius: 4px;
    padding: 5px 8px;
    font: inherit;
  }
  .warn {
    color: var(--bad);
    font-size: 12px;
  }
  .note {
    font-size: 12px;
  }
</style>
