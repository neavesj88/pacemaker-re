<script lang="ts">
  import type { Mode, Rule } from "$lib/api/cases";
  import { MAX_DEPTH, MODES, defaultCaseId, defaultRule, describeRule, newGroup } from "$lib/state/smartRules";
  import { cases } from "$lib/state/cases.svelte";
  import Button from "$lib/ui/Button.svelte";
  import ConfirmDialog from "./ConfirmDialog.svelte";
  import Self from "./SmartRuleGroup.svelte";
  import SmartRuleRow from "./SmartRuleRow.svelte";

  // A list of rules combined by a mode (all / any / none); a group inside it is another of these.
  // `depth` is 1 for the smart case's own list, as in the backend's nesting limit.
  let {
    mode = $bindable(),
    items = $bindable(),
    depth = 1,
    onremove,
  }: { mode: Mode; items: Rule[]; depth?: number; onremove?: () => void } = $props();

  const lead = $derived(depth === 1 ? "Tracks that match" : "Matches");
  const addRule = () => items.push(defaultRule("plays_zero", defaultCaseId(cases.infos)));
  const addGroup = () => items.push(newGroup(defaultRule("plays_zero")));
  // Removing a group takes every rule inside it, so it is confirmed with the rules listed in words.
  let removing = $state(false);
  const caseName = (cid: number) => cases.infos.find((c) => c.id === cid)?.name ?? `missing case ${cid}`;
  const describe = (r: Rule): string => {
    try {
      return describeRule(r, caseName);
    } catch {
      return "a rule that is not finished";
    }
  };
  const countRules = (list: Rule[]): number => list.reduce((n, r) => n + (r.kind === "group" ? countRules(r.items) : 1), 0);
</script>

<div class="group" class:nested={depth > 1}>
  <div class="head">
    <span class="muted">{lead}</span>
    <select bind:value={mode} aria-label="How the rules in this group combine">
      {#each MODES as m (m.mode)}<option value={m.mode}>{m.label}</option>{/each}
    </select>
    {#if onremove}
      <Button small variant="ghost" onclick={() => (removing = true)} aria-label="Remove this group and its rules">Remove group ({countRules(items)} rule{countRules(items) === 1 ? "" : "s"})</Button>
    {/if}
  </div>
  <div class="items">
    {#each items as item, i (item)}
      {#if item.kind === "group"}
        <Self bind:mode={item.mode} bind:items={item.items} depth={depth + 1} onremove={() => items.splice(i, 1)} />
      {:else}
        <SmartRuleRow bind:rule={items[i]} onremove={() => items.splice(i, 1)} />
      {/if}
    {/each}
    <div class="add">
      <Button small onclick={addRule}>+ Add rule</Button>
      {#if depth < MAX_DEPTH}<Button small onclick={addGroup}>+ Add group</Button>{/if}
    </div>
  </div>
</div>

{#if removing && onremove}
  <ConfirmDialog
    title="Remove this group?"
    message={`Remove this group and the ${countRules(items)} rule${countRules(items) === 1 ? "" : "s"} in it from the smart case? Nothing is saved until you press Save, and leaving without saving discards the change.`}
    items={items.map(describe)}
    confirmLabel="Remove group"
    onconfirm={onremove}
    onclose={() => (removing = false)}
  />
{/if}

<style>
  .group {
    display: grid;
    gap: 6px;
  }
  .group.nested {
    padding: 8px 10px;
    border: 1px solid var(--line-hi);
    border-left: 3px solid var(--accent);
    border-radius: 6px;
    background: var(--deep);
  }
  .head {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }
  .items {
    display: grid;
    gap: 6px;
  }
  .nested > .items {
    padding-left: 6px;
  }
  .add {
    display: flex;
    gap: 8px;
  }
</style>
