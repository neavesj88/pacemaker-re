<script lang="ts">
  import { session } from "$lib/state/session.svelte";
  // The smart-case choices: whether the Pacemaker copies refresh by themselves, and how smart
  // cases appear in exports. Saved in the app's own config (not on the device).
  import type { RefreshMode } from "$lib/api/cases";
  import { smartSettings } from "$lib/state/smartSettings.svelte";
  import Button from "$lib/ui/Button.svelte";
  import Modal from "$lib/ui/Modal.svelte";

  let { onclose }: { onclose: () => void } = $props();

  const MODES: { value: RefreshMode; label: string; note: string }[] = [
    { value: "ask", label: "Ask me", note: "When the library is opened or edited and a copy no longer matches its rules, show what would change and wait for a yes." },
    { value: "auto", label: "Do it automatically", note: "Rewrite the copy straight away, then say which smart cases changed and how." },
    { value: "off", label: "Only when I press “Refresh smart cases”", note: "Nothing is written by itself." },
  ];
  const s = $derived(smartSettings.value);
  $effect(() => {
    void smartSettings.ensure();
  });
</script>

<Modal title="Smart case settings" {onclose}>
  {#if smartSettings.warning}<p class="warn" role="alert">{smartSettings.warning}</p>{/if}
  <fieldset>
    <legend>{session.isComputer ? 'The ★ cases' : 'Pacemaker copies (the ★ cases)'} of smart cases that are switched on</legend>
    {#each MODES as m (m.value)}
      <label class="opt">
        <input type="radio" name="refresh-mode" value={m.value} checked={s.refresh_mode === m.value} onchange={() => smartSettings.set({ refresh_mode: m.value })} />
        <span><strong>{m.label}</strong><span class="muted note">{m.note}</span></span>
      </label>
    {/each}
    <p class="muted">Writes go through the same safety as every other change to the device. Nothing is written when every copy is already current.</p>
  </fieldset>
  <fieldset>
    <legend>Exports</legend>
    <label class="opt">
      <input type="checkbox" checked={s.export_smart} onchange={(e) => smartSettings.set({ export_smart: e.currentTarget.checked })} />
      <span>
        <strong>Include smart cases in the rekordbox XML of the whole library</strong>
        <span class="muted note">Each becomes an ordinary playlist of the tracks it matches when you export (rekordbox XML cannot hold rules). Their ★ copies are not written a second time. M3U8, CSV and XML of a single smart case are always available.</span>
      </span>
    </label>
    <label class="opt">
      <input type="checkbox" checked={s.export_folder} disabled={!s.export_smart} onchange={(e) => smartSettings.set({ export_folder: e.currentTarget.checked })} />
      <span><strong>Put them in a folder called “Smart cases”</strong><span class="muted note">Off: they sit beside the ordinary cases.</span></span>
    </label>
  </fieldset>
  {#snippet actions()}
    <Button data-autofocus onclick={onclose}>Close</Button>
  {/snippet}
</Modal>

<style>
  fieldset {
    margin: 0 0 10px;
    padding: 8px 10px;
    border: 1px solid var(--line);
    border-radius: 4px;
  }
  legend {
    padding: 0 4px;
    color: var(--muted);
    font-size: 12px;
  }
  .opt {
    display: flex;
    gap: 8px;
    align-items: flex-start;
    margin: 4px 0;
    cursor: pointer;
  }
  .opt input {
    margin-top: 3px;
  }
  .note {
    display: block;
    font-size: 12px;
  }
  p {
    margin: 6px 0 0;
  }
  .warn {
    color: var(--bad-text);
  }
</style>
