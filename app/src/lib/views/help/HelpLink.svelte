<script lang="ts">
  // A small "?" that opens one Help article. Put it next to the thing the article explains:
  // <HelpLink id="install-firmware" />. The id is the article's stable id.
  import { helpIndex } from "./articles";
  import { openHelp } from "./help.svelte";

  let { id, anchor = null }: { id: string; anchor?: string | null } = $props();
  const title = $derived(helpIndex().byId.get(id)?.article.title ?? "Help");
</script>

<button type="button" class="q" aria-label="Help: {title}" title="Help: {title}" onclick={(e) => (e.stopPropagation(), openHelp(id, { anchor }))}>?</button>

<style>
  .q {
    flex: none;
    width: 20px;
    height: 20px;
    padding: 0;
    border: 1px solid var(--line-hi);
    border-radius: 50%;
    background: none;
    color: var(--muted);
    font: inherit;
    font-size: 12px;
    line-height: 1;
    cursor: pointer;
  }
  .q:hover,
  .q:focus-visible {
    color: var(--accent);
    border-color: var(--accent);
  }
</style>
