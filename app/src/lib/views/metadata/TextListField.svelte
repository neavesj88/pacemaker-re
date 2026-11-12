<script lang="ts">
  import { untrack } from "svelte";
  import type { MapEntry } from "$lib/api/metacleanup";
  import { formatList, formatMap, parseList, parseMap } from "./cleanupLogic";

  // A list (one entry per line) or a map ("from -> to" per line) edited as plain text. The text
  // is kept as typed; the parsed value goes up on every keystroke, and an outside change (Reset
  // to defaults) replaces the text only when it no longer says the same thing.
  let {
    kind,
    value,
    onchange,
    label,
    hint = "",
    rows = 4,
  }: {
    kind: "list" | "map";
    value: string[] | MapEntry[];
    onchange: (v: any) => void;
    label: string;
    hint?: string;
    rows?: number;
  } = $props();

  const fmt = (v: string[] | MapEntry[]) => (kind === "map" ? formatMap(v as MapEntry[]) : formatList(v as string[]));
  const parse = (t: string): { entries: string[] | MapEntry[]; ignored: number } =>
    kind === "map" ? parseMap(t) : { entries: parseList(t), ignored: 0 };

  let text = $state(untrack(() => fmt(value)));
  let ignored = $state(0);

  $effect(() => {
    const incoming = JSON.stringify(value);
    if (JSON.stringify(parse(untrack(() => text)).entries) !== incoming) text = fmt(value);
  });

  function oninput(e: Event & { currentTarget: HTMLTextAreaElement }) {
    text = e.currentTarget.value;
    const p = parse(text);
    ignored = p.ignored;
    onchange(p.entries);
  }
</script>

<label class="f">
  <span class="lbl">{label}</span>
  <textarea {rows} value={text} {oninput} spellcheck="false" autocomplete="off"></textarea>
  {#if hint}<span class="hint">{hint}</span>{/if}
  {#if ignored > 0}
    <span class="warn" role="status">
      {ignored} {ignored === 1 ? "line has" : "lines have"} no “->” between two names and {ignored === 1 ? "is" : "are"} ignored.
    </span>
  {/if}
</label>

<style>
  .f {
    display: flex;
    flex-direction: column;
    gap: 3px;
    min-width: 0;
  }
  .lbl {
    font-size: 12px;
    color: var(--muted);
  }
  textarea {
    width: 100%;
    box-sizing: border-box;
    padding: 5px 8px;
    background: var(--deep);
    color: var(--text);
    border: 1px solid var(--line);
    border-radius: 4px;
    font: inherit;
    resize: vertical;
  }
  textarea:focus {
    outline: none;
    border-color: var(--accent);
  }
  .hint {
    font-size: 11px;
    color: var(--muted);
  }
  .warn {
    font-size: 11px;
    color: var(--bad-text);
  }
</style>
