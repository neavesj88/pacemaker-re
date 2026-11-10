<script lang="ts">
  import { untrack } from "svelte";
  import { deviceSettingsWrite, type SettingView } from "$lib/api/device";
  import { jobs } from "$lib/state/jobs.svelte";
  import { session } from "$lib/state/session.svelte";
  import { toast } from "$lib/state/toast.svelte";
  import Button from "$lib/ui/Button.svelte";
  import Modal from "$lib/ui/Modal.svelte";
  import Card from "./Card.svelte";
  import { device } from "./device.svelte";
  import { plural } from "./format";

  // Edits are collected here and only written after a confirmation that names every change.
  let edits = $state<Record<number, number>>({});
  let confirming = $state(false);
  let saving = $state(false);

  // edits belong to the drive they were made on
  $effect(() => {
    void session.library?.root;
    untrack(() => {
      edits = {};
      confirming = false;
    });
  });

  let view = $derived(device.settings);
  let groups = $derived.by(() => {
    const map = new Map<string, SettingView[]>();
    for (const s of view?.settings ?? []) {
      if (!s.present) continue;
      if (!map.has(s.group)) map.set(s.group, []);
      map.get(s.group)!.push(s);
    }
    return [...map];
  });
  let absent = $derived((view?.settings ?? []).filter((s) => !s.present).length);
  let open = new Set(["Playback", "Mix control", "Power"]);

  let changes = $derived(
    (view?.settings ?? [])
      .filter((s) => s.id in edits)
      .map((s) => ({ s, from: label(s, s.value), to: label(s, edits[s.id]) })),
  );

  function label(s: SettingView, v: number): string {
    if (s.kind === "float") return String(v);
    return s.options[v] ?? `#${v}`;
  }

  function set(s: SettingView, v: number) {
    if (v === s.value) delete edits[s.id];
    else edits[s.id] = v;
  }

  async function save() {
    const lib = session.library;
    if (!lib) return;
    saving = true;
    try {
      const n = await deviceSettingsWrite(lib.root, changes.map((c) => ({ id: c.s.id, value: edits[c.s.id] })));
      toast.show(`${plural(n, "setting")} written. The Pacemaker reads them when it next starts.`, "ok", 6000);
      edits = {};
      confirming = false;
      await device.load(lib.root);
    } catch (e) {
      confirming = false;
      toast.show(String(e), "bad", 8000);
    } finally {
      saving = false;
    }
  }
</script>

<Card title="Device settings">
  {#if view === undefined}
    <p class="muted">Reading the drive…</p>
  {:else if view === null}
    <p class="muted">
      This drive has no settings file yet. The Pacemaker creates it the first time it is shut down.
    </p>
  {:else}
    {#each groups as [group, list] (group)}
      <details open={open.has(group)}>
        <summary>{group}</summary>
        {#each list as s (s.id)}
          <div class="field" class:edited={s.id in edits}>
            <label for={s.editable ? `set-${s.id}` : undefined} id="lbl-{s.id}">{s.title}</label>
            {#if s.editable}
              <select
                id="set-{s.id}"
                value={edits[s.id] ?? s.value}
                onchange={(e) => set(s, Number(e.currentTarget.value))}
                disabled={saving}
              >
                {#each s.options as o, i (i)}<option value={i}>{o}</option>{/each}
              </select>
            {:else}
              <span class="num ro" role="group" aria-labelledby="lbl-{s.id}">{s.value.toPrecision(5)}</span>
            {/if}
            {#if s.changed && !(s.id in edits)}<span class="tag" title="Differs from the factory default">changed</span>{/if}
          </div>
        {/each}
      </details>
    {/each}
    <div class="note">
      Edits the Pacemaker's settings.dat directly. {#if absent}{plural(absent, "setting")} the file does not hold yet
        (the device adds them when it saves) cannot be edited here.{/if}
      Levels and EQ are raw engine values with unknown ranges, so they are shown but not editable.
    </div>
    <div class="actions">
      <Button
        variant="primary"
        disabled={changes.length === 0 || saving || device.blocked}
        title={jobs.busy ? "Another job is still running" : undefined}
        onclick={() => (confirming = true)}
      >
        Write {changes.length ? plural(changes.length, "change") : "changes"}…
      </Button>
      {#if changes.length}<Button variant="ghost" onclick={() => (edits = {})}>Discard</Button>{/if}
    </div>
  {/if}
</Card>

{#if confirming}
  <Modal title="Write these settings to the Pacemaker?" onclose={() => (confirming = false)}>
    <ul class="changes">
      {#each changes as c (c.s.id)}
        <li><b>{c.s.group}: {c.s.title}</b> {c.from} → {c.to}</li>
      {/each}
    </ul>
    <p class="muted">
      This rewrites .Pacemaker/settings.dat on {session.library?.root}. Only the values above change; the rest of the
      file is written back exactly as it was. Back up first if you want a way back. Do not write while the device is
      running: eject, then power it on.
    </p>
    {#snippet actions()}
      <Button data-autofocus onclick={() => (confirming = false)}>Cancel</Button>
      <Button variant="primary" disabled={saving} onclick={save}>{saving ? "Writing…" : "Write settings"}</Button>
    {/snippet}
  </Modal>
{/if}

<style>
  details {
    border-top: 1px solid var(--line);
    padding: 4px 0;
  }
  summary {
    cursor: pointer;
    color: var(--muted);
    padding: 3px 0;
  }
  .field {
    display: grid;
    grid-template-columns: 1fr 150px auto;
    gap: 6px;
    align-items: center;
    margin: 4px 0 4px 12px;
  }
  .field label {
    color: var(--text);
    font-size: 12px;
  }
  .field.edited label {
    color: var(--accent);
  }
  .field.edited select {
    border-color: var(--accent);
  }
  .ro {
    color: var(--muted);
    text-align: right;
  }
  .tag {
    font-size: 11px;
    padding: 0 6px;
    border-radius: 8px;
    background: var(--accent-soft);
    color: var(--accent);
  }
  .note {
    font-size: 12px;
    color: var(--muted);
    border-top: 1px solid var(--line);
    margin-top: 8px;
    padding-top: 10px;
  }
  .actions {
    margin-top: 10px;
    display: flex;
    gap: 8px;
  }
  .changes {
    margin: 8px 0;
    padding-left: 18px;
  }
</style>
