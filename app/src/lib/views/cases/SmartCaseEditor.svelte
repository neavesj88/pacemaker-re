<script lang="ts">
  import { evaluateRules, type Rule, type SmartCase } from "$lib/api/cases";
  import type { Track } from "$lib/api/types";
  import { cases } from "$lib/state/cases.svelte";
  import { jobs } from "$lib/state/jobs.svelte";
  import { session } from "$lib/state/session.svelte";
  import { toast } from "$lib/state/toast.svelte";
  import Button from "$lib/ui/Button.svelte";
  import TrackTable from "../tracks/TrackTable.svelte";
  import { visibleTracks } from "../tracks/trackFilter";
  import ConfirmDialog from "./ConfirmDialog.svelte";

  // The rule builder and the matching tracks of one smart case (or a new one, id 0). Rules are
  // saved in the app's config dir; nothing reaches the device until "Put on Pacemaker" is switched on.
  const id = cases.smartOpen ?? 0;

  const KINDS: { kind: Rule["kind"]; label: string }[] = [
    { kind: "rating_min", label: "Rating at least" },
    { kind: "favourite", label: "Favourites" },
    { kind: "plays_zero", label: "Never played" },
    { kind: "plays_min", label: "Played at least" },
    { kind: "played_within", label: "Last played within" },
    { kind: "bpm_range", label: "BPM between" },
    { kind: "key", label: "Key is" },
    { kind: "key_compatible", label: "Key compatible with" },
    { kind: "genre", label: "Genre contains" },
    { kind: "analysed", label: "Analysed" },
  ];

  function defaultRule(kind: string): Rule {
    switch (kind) {
      case "rating_min": return { kind, stars: 4 };
      case "favourite": return { kind, value: true };
      case "plays_min": return { kind, n: 5 };
      case "played_within": return { kind, days: 30 };
      case "bpm_range": return { kind, min: 120, max: 130 };
      case "key": return { kind, key: "8A" };
      case "key_compatible": return { kind, key: "8A" };
      case "genre": return { kind, text: "" };
      case "analysed": return { kind, value: true };
      default: return { kind: "plays_zero" };
    }
  }

  const stored = (): SmartCase | undefined => cases.smart.find((s) => s.id === id);
  const start: SmartCase = stored()
    ? JSON.parse(JSON.stringify(stored()))
    : { id: 0, name: "New smart case", rules: [defaultRule("rating_min")], on_device: false, device_case_id: null };

  let draft = $state<SmartCase>(start);
  let baseline = $state(JSON.stringify({ name: start.name, rules: start.rules }));
  let dirty = $derived(JSON.stringify({ name: draft.name, rules: draft.rules }) !== baseline);
  let sc = $derived(cases.smart.find((s) => s.id === id));

  // ---- live matches -----------------------------------------------------------------------
  let matchIds = $state<number[]>([]);
  let ruleError = $state("");
  let counting = $state(false);
  let ticket = 0;

  $effect(() => {
    const rules = $state.snapshot(draft.rules) as Rule[];
    const root = session.library?.root;
    // library tracks are read so a reload (a refresh after a write) re-runs the count
    void session.library?.tracks;
    if (!root) return;
    const mine = ++ticket;
    counting = true;
    const timer = setTimeout(async () => {
      try {
        const ids = await evaluateRules(root, rules);
        if (mine === ticket) (matchIds = ids, (ruleError = ""));
      } catch (e) {
        if (mine === ticket) (matchIds = [], (ruleError = String(e)));
      } finally {
        if (mine === ticket) counting = false;
      }
    }, 150);
    return () => clearTimeout(timer);
  });

  let rows = $derived.by(() => {
    const byId = new Map((session.library?.tracks ?? []).map((t) => [t.track_id, t]));
    return visibleTracks(matchIds.map((m) => byId.get(m)).filter((t): t is Track => !!t));
  });

  // ---- actions ----------------------------------------------------------------------------
  let deleting = $state(false);
  let alsoDevice = $state(false);
  let takingOff = $state(false);

  const deviceName = (name: string) => `★ ${name.trim()}`;

  async function save() {
    const out = await cases.saveSmart($state.snapshot(draft) as SmartCase);
    if (!out) return;
    baseline = JSON.stringify({ name: out.name, rules: out.rules });
    toast.show(`Saved “${out.name}”`, "ok");
    if (id === 0) cases.openSmart(out.id);
  }

  const addRule = () => draft.rules.push(defaultRule("plays_zero"));
  const removeRule = (i: number) => draft.rules.splice(i, 1);
</script>

<div class="crumb">
  <Button small onclick={() => cases.close()}>← Cases</Button>
  <span class="muted">Smart case</span>
  <input class="name" bind:value={draft.name} aria-label="Smart case name" maxlength="200" />
  <Button variant="primary" onclick={save} disabled={!dirty || draft.name.trim() === "" || draft.rules.length === 0 || !!ruleError}>
    {id === 0 ? "Create" : "Save"}
  </Button>
  {#if id !== 0}<Button onclick={() => ((alsoDevice = false), (deleting = true))}>Delete…</Button>{/if}
</div>

<div class="builder">
  <div class="rules">
    <div class="lead muted">Tracks that match all of these:</div>
    {#each draft.rules as rule, i (i)}
      <div class="rule">
        <select
          value={rule.kind}
          aria-label="Rule type"
          onchange={(e) => (draft.rules[i] = defaultRule(e.currentTarget.value))}
        >
          {#each KINDS as k (k.kind)}<option value={k.kind}>{k.label}</option>{/each}
        </select>
        {#if rule.kind === "rating_min"}
          <select bind:value={rule.stars} aria-label="Stars">
            {#each [1, 2, 3, 4, 5] as n (n)}<option value={n}>{"★".repeat(n)}</option>{/each}
          </select>
        {:else if rule.kind === "favourite"}
          <select bind:value={rule.value} aria-label="In Favourites">
            <option value={true}>are in Favourites</option>
            <option value={false}>are not in Favourites</option>
          </select>
        {:else if rule.kind === "plays_min"}
          <input type="number" min="1" bind:value={rule.n} aria-label="Plays" /> <span class="muted">times</span>
        {:else if rule.kind === "played_within"}
          <input type="number" min="1" bind:value={rule.days} aria-label="Days" /> <span class="muted">days</span>
        {:else if rule.kind === "bpm_range"}
          <input type="number" min="0" step="0.5" placeholder="any" bind:value={rule.min} aria-label="Minimum BPM" />
          <span class="muted">to</span>
          <input type="number" min="0" step="0.5" placeholder="any" bind:value={rule.max} aria-label="Maximum BPM" />
        {:else if rule.kind === "key" || rule.kind === "key_compatible"}
          <input class="key" bind:value={rule.key} placeholder="8A or Am" aria-label="Key" />
          {#if rule.kind === "key_compatible"}<span class="muted">same, next to it on the wheel, or its relative</span>{/if}
        {:else if rule.kind === "genre"}
          <input bind:value={rule.text} placeholder="e.g. house" aria-label="Genre text" />
        {:else if rule.kind === "analysed"}
          <select bind:value={rule.value} aria-label="Analysed">
            <option value={true}>have been analysed</option>
            <option value={false}>have not been analysed</option>
          </select>
        {/if}
        <Button small variant="ghost" onclick={() => removeRule(i)} aria-label="Remove this rule">Remove</Button>
      </div>
    {/each}
    <Button small onclick={addRule}>+ Add rule</Button>
  </div>

  <aside class="side">
    <div class="count" aria-live="polite">
      {#if ruleError}
        <span class="err">{ruleError}</span>
      {:else}
        <b>{matchIds.length}</b> track{matchIds.length === 1 ? "" : "s"} match{counting ? "…" : ""}
      {/if}
    </div>

    <div class="device">
      <div class="row">
        <button
          class="switch"
          role="switch"
          aria-checked={sc?.on_device ?? false}
          aria-label="Put on Pacemaker"
          disabled={id === 0 || dirty || !!ruleError || !jobs.canWrite}
          onclick={() => (sc?.on_device ? (takingOff = true) : cases.setSmartOnDevice(id, true))}
        ><span></span></button>
        <b>Put on Pacemaker</b>
      </div>
      <p class="muted">
        {#if id === 0}
          Save the smart case first.
        {:else if dirty}
          Save your changes first: the Pacemaker copy is made from the saved rules.
        {:else if sc?.on_device}
          The Pacemaker has a case called “{deviceName(sc.name)}” with these tracks. It does not change by itself:
          refresh it after the library changes.
        {:else}
          Off: this smart case exists only in this app. Switch it on to write a normal case called “{deviceName(draft.name)}” to the Pacemaker.
        {/if}
      </p>
      {#if sc?.on_device && !dirty}
        <Button small disabled={!jobs.canWrite || !!ruleError} onclick={() => cases.setSmartOnDevice(id, true)}>Refresh on Pacemaker now</Button>
      {/if}
    </div>
  </aside>
</div>

<TrackTable {rows} />
{#if rows.length === 0 && !ruleError}
  <p class="empty">No tracks match these rules.</p>
{/if}

{#if takingOff && sc}
  <ConfirmDialog
    title="Remove from the Pacemaker?"
    message={`Delete the case “${deviceName(sc.name)}” from the Pacemaker? The smart case stays in this app and no track is deleted; you can put it back at any time.`}
    confirmLabel="Delete device case"
    onconfirm={() => cases.setSmartOnDevice(id, false)}
    onclose={() => (takingOff = false)}
  />
{/if}

{#if deleting && sc}
  <ConfirmDialog
    title="Delete this smart case?"
    message={`Delete the smart case “${sc.name}” (${cases.smartCounts[sc.id] ?? matchIds.length} matching tracks) from this app? No track is deleted.`}
    confirmLabel="Delete smart case"
    onconfirm={() => cases.deleteSmart(id, alsoDevice)}
    onclose={() => (deleting = false)}
  >
    {#snippet extra()}
      {#if sc.on_device}
        <label class="also">
          <input type="checkbox" bind:checked={alsoDevice} />
          Also delete the case “{deviceName(sc.name)}” from the Pacemaker (otherwise it stays there as an ordinary case)
        </label>
      {/if}
    {/snippet}
  </ConfirmDialog>
{/if}

<style>
  .crumb {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 10px;
    border-bottom: 1px solid var(--line);
    flex-wrap: wrap;
  }
  .name {
    min-width: 220px;
    font-weight: 600;
  }
  .builder {
    display: flex;
    gap: 16px;
    padding: 10px;
    border-bottom: 1px solid var(--line);
    flex-wrap: wrap;
  }
  .rules {
    flex: 1;
    min-width: 360px;
    display: grid;
    gap: 6px;
    align-content: start;
  }
  .rule {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }
  .rule input[type="number"] {
    width: 72px;
  }
  .key {
    width: 100px;
  }
  .side {
    width: 300px;
    display: grid;
    gap: 12px;
    align-content: start;
  }
  .count {
    font-size: 14px;
  }
  .count b {
    font-size: 20px;
    color: var(--green);
  }
  .err {
    color: var(--bad);
  }
  .device {
    padding: 10px;
    background: var(--panel);
    border: 1px solid var(--line);
    border-radius: 6px;
  }
  .device p {
    margin: 6px 0;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .switch {
    width: 34px;
    height: 18px;
    padding: 2px;
    border-radius: 9px;
    border: 1px solid var(--line-hi);
    background: var(--deep);
    cursor: pointer;
    display: flex;
    align-items: center;
  }
  .switch span {
    width: 12px;
    height: 12px;
    border-radius: 50%;
    background: var(--muted);
    transition: transform 0.12s;
  }
  .switch[aria-checked="true"] {
    background: var(--accent-soft);
    border-color: var(--accent);
  }
  .switch[aria-checked="true"] span {
    background: var(--accent);
    transform: translateX(16px);
  }
  .switch:disabled {
    opacity: 0.45;
    cursor: default;
  }
  .also {
    display: flex;
    gap: 8px;
    align-items: flex-start;
    margin-bottom: 8px;
  }
  .also input {
    margin-top: 3px;
  }
</style>
