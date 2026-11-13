<script lang="ts">
  import { diffParts } from "./cleanupLogic";

  // One side of a before / after pair, with the part that differs marked (also visible when it
  // is only white space: the marked part has a background, and empty text reads "(empty)").
  let { old: before, new: after, side }: { old: string; new: string; side: "old" | "new" } = $props();
  const p = $derived(diffParts(before, after));
  const mid = $derived(side === "old" ? p.del : p.add);
</script>

<span class="t">{p.pre}{#if mid}<mark class={side}>{mid}</mark>{/if}{p.post}{#if !p.pre && !mid && !p.post}<span class="e">(empty)</span>{/if}</span>

<style>
  .t {
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
  mark {
    color: inherit;
    border-radius: 2px;
    padding: 0 1px;
  }
  mark.old {
    background: var(--bad-bg);
    color: var(--bad-text);
    text-decoration: line-through;
  }
  mark.new {
    background: color-mix(in srgb, var(--green) 18%, transparent);
  }
  .e {
    color: var(--muted);
    font-style: italic;
  }
</style>
