<script lang="ts">
  import { untrack } from "svelte";
  import {
    devicePresetImport,
    devicePresetSaveCurrent,
    type Preset,
    type PresetSummary,
    type SaveResult,
  } from "$lib/api/device";
  import { session } from "$lib/state/session.svelte";
  import { toast } from "$lib/state/toast.svelte";
  import Button from "$lib/ui/Button.svelte";
  import { device } from "./device.svelte";
  import { plural } from "./format";
  import { groupRows, nameProblem, presetMeta, sameName } from "./presetLogic";
  import { stampLabel } from "./trustLogic";
  import WideModal from "./WideModal.svelte";

  // Names a preset and stores it: either the settings of the card that is open ("save") or a
  // preset file the user picked ("import", `file` is what the file holds, shown in full). A name
  // that is already taken is never overwritten without asking, and the question names the
  // preset that would be replaced.
  let {
    mode,
    existing,
    file,
    path,
    onclose,
    ondone,
  }: {
    mode: "save" | "import";
    /** The stored presets, to name the one a replacement would overwrite. */
    existing: PresetSummary[];
    /** import: what the file holds. */
    file?: Preset;
    /** import: the file's path. */
    path?: string;
    onclose: () => void;
    ondone: (r: SaveResult) => void;
  } = $props();

  // the dialog is mounted per use, so the starting name is read once
  let name = $state(untrack(() => (mode === "import" ? (file?.name ?? "") : "")));
  let confirmReplace = $state(false);
  let busy = $state(false);

  const problem = $derived(name.trim() === "" ? "" : nameProblem(name));
  const taken = $derived(existing.find((p) => sameName(p.name, name)));
  const cardCount = $derived((device.settings?.settings ?? []).filter((s) => s.present && s.editable).length);
  const groups = $derived(groupRows(file?.settings ?? []));

  $effect(() => {
    void name;
    confirmReplace = false;
  });

  async function submit(overwrite: boolean) {
    const lib = session.library;
    if (nameProblem(name) || busy) return;
    busy = true;
    try {
      const r =
        mode === "save"
          ? await devicePresetSaveCurrent(lib?.root ?? "", name, overwrite)
          : await devicePresetImport(path ?? "", name, overwrite);
      if (r.exists) {
        confirmReplace = true;
        return;
      }
      ondone(r);
    } catch (e) {
      toast.show(String(e), "bad", 8000);
    } finally {
      busy = false;
    }
  }
</script>

<WideModal title={mode === "save" ? "Save this card's settings as a preset" : "Add a preset from a file"} {onclose} width={560}>
  {#if mode === "save"}
    <p class="muted">
      Saves the values of the {plural(cardCount, "editable setting")} in this card's settings.dat under a name. The read-only
      engine levels are not saved. The preset is kept on this PC, not on the Pacemaker.
    </p>
  {:else if file}
    <p class="muted">
      {path}: {presetMeta({ settings: file.settings.length, firmware: file.firmware }, file.saved_utc ? stampLabel(file.saved_utc) : "")}.
      Adding it only stores it here; nothing is written to a card until you apply it.
    </p>
    <div class="box" aria-label="Settings in the file">
      {#each groups as g (g.group)}
        <div class="grp">{g.group}</div>
        {#each g.rows as s (s.id)}
          <div class="row"><span>{s.title}</span><span class="v">{s.label}</span></div>
        {/each}
      {/each}
    </div>
  {/if}
  <label class="nm" for="preset-name">Name</label>
  <input
    id="preset-name"
    data-autofocus
    type="text"
    maxlength="120"
    bind:value={name}
    disabled={busy}
    onkeydown={(e) => e.key === "Enter" && !confirmReplace && submit(false)}
  />
  {#if problem}<p class="bad">{problem}</p>{/if}
  {#if confirmReplace && taken}
    <p class="warn" role="alert">
      A preset called "{taken.name}" is already stored ({presetMeta(taken, taken.saved_utc ? stampLabel(taken.saved_utc) : "")}).
      Replacing it deletes its settings; choose another name to keep both.
    </p>
  {/if}
  {#snippet actions()}
    <Button onclick={onclose}>Cancel</Button>
    {#if confirmReplace}
      <Button variant="primary" disabled={busy} onclick={() => submit(true)}>Replace "{taken?.name ?? name}"</Button>
    {:else}
      <Button variant="primary" disabled={busy || name.trim() === "" || !!problem || (mode === "save" && !session.library)} onclick={() => submit(false)}>
        {mode === "save" ? "Save preset" : "Add preset"}
      </Button>
    {/if}
  {/snippet}
</WideModal>

<style>
  .nm {
    display: block;
    margin: 12px 0 3px;
    font-size: 12px;
    color: var(--muted);
  }
  input[type="text"] {
    width: 100%;
    box-sizing: border-box;
  }
  .box {
    max-height: 260px;
    overflow: auto;
    border: 1px solid var(--line);
    border-radius: 4px;
    padding: 4px 8px;
  }
  .grp {
    margin-top: 6px;
    font-size: 12px;
    color: var(--muted);
    font-weight: 600;
  }
  .row {
    display: flex;
    justify-content: space-between;
    gap: 12px;
    padding: 1px 0;
  }
  .v {
    color: var(--accent);
    text-align: right;
  }
  .bad {
    color: var(--bad-text);
    font-size: 12px;
    margin: 4px 0 0;
  }
  .warn {
    margin: 10px 0 0;
    padding: 8px 10px;
    border: 1px solid var(--bad);
    background: var(--bad-bg);
    color: var(--bad-text);
    border-radius: 4px;
    font-size: 12px;
  }
</style>
