<script lang="ts">
  // Draws the blocks the reader (helpLogic.ts) made. Every piece is text: nothing here is {@html}.
  import type { Block, Inline, ListBlock } from "./helpLogic";
  import { helpImage } from "./articles";

  let {
    blocks,
    known,
    onhelp,
  }: {
    blocks: Block[];
    /** Whether an article id exists; a link to one that does not is drawn as text. */
    known: (id: string) => boolean;
    onhelp: (id: string, anchor: string | null) => void;
  } = $props();
</script>

{#snippet inl(list: Inline[])}
  {#each list as n, i (i)}
    {#if n.t === "text"}{n.text}
    {:else if n.t === "strong"}<strong>{n.text}</strong>
    {:else if n.t === "code"}<code>{n.text}</code>
    {:else if n.t === "help"}
      {#if known(n.id)}<button type="button" class="xref" onclick={() => onhelp(n.id, n.anchor)}>{n.text}</button>{:else}{n.text}{/if}
    {:else}{n.text} <span class="url">({n.url})</span>
    {/if}
  {/each}
{/snippet}

{#snippet list(b: ListBlock)}
  {#if b.t === "ul"}
    <ul>
      {#each b.items as it, ii (ii)}
        <li>{@render inl(it)}{#if b.subs[ii]}{@render list(b.subs[ii]!)}{/if}</li>
      {/each}
    </ul>
  {:else}
    <ol start={b.start}>
      {#each b.items as it, ii (ii)}
        <li>{@render inl(it)}{#if b.subs[ii]}{@render list(b.subs[ii]!)}{/if}</li>
      {/each}
    </ol>
  {/if}
{/snippet}

{#each blocks as b, bi (bi)}
  {#if b.t === "h"}
    {#if b.level === 2}<h2 id="h-{b.slug}">{b.text}</h2>
    {:else if b.level === 3}<h3 id="h-{b.slug}">{b.text}</h3>
    {:else}<h4 id="h-{b.slug}">{b.text}</h4>{/if}
  {:else if b.t === "p"}
    <p>{@render inl(b.inl)}</p>
  {:else if b.t === "note"}
    <p class="note">{@render inl(b.inl)}</p>
  {:else if b.t === "ul" || b.t === "ol"}
    {@render list(b)}
  {:else if b.t === "code"}
    <pre><code>{b.text}</code></pre>
  {:else if b.t === "img"}
    {@const url = helpImage(b.src)}
    {#if url}<figure><img src={url} alt={b.alt} loading="lazy" />{#if b.alt}<figcaption>{b.alt}</figcaption>{/if}</figure>{/if}
  {:else if b.t === "table"}
    <div class="tbl">
      <table>
        <thead><tr>{#each b.head as c, ci (ci)}<th>{@render inl(c)}</th>{/each}</tr></thead>
        <tbody>
          {#each b.rows as r, ri (ri)}<tr>{#each r as c, ci (ci)}<td>{@render inl(c)}</td>{/each}</tr>{/each}
        </tbody>
      </table>
    </div>
  {/if}
{/each}

<style>
  h2 {
    margin: 22px 0 6px;
    font-size: 16px;
    scroll-margin-top: 8px;
  }
  h3 {
    margin: 16px 0 4px;
    font-size: 14px;
    scroll-margin-top: 8px;
  }
  h4 {
    margin: 12px 0 4px;
    font-size: 13px;
    color: var(--muted);
    scroll-margin-top: 8px;
  }
  p {
    margin: 0 0 10px;
    line-height: 1.55;
  }
  ul,
  ol {
    margin: 0 0 10px;
    padding-left: 22px;
    line-height: 1.55;
  }
  li {
    margin: 2px 0;
  }
  li > ul,
  li > ol {
    margin: 3px 0 4px;
  }
  ol li::marker {
    color: var(--accent);
    font-weight: 600;
  }
  code {
    font-family: ui-monospace, "Cascadia Mono", Consolas, monospace;
    font-size: 0.92em;
    padding: 0 4px;
    border-radius: 3px;
    background: var(--row-alt);
    color: var(--green);
  }
  pre {
    margin: 0 0 10px;
    padding: 8px 10px;
    overflow-x: auto;
    background: var(--deep);
    border: 1px solid var(--line);
    border-radius: 4px;
  }
  pre code {
    padding: 0;
    background: none;
  }
  .note {
    padding: 8px 12px;
    border-left: 3px solid var(--accent);
    background: var(--accent-soft);
    border-radius: 0 4px 4px 0;
  }
  figure {
    margin: 4px 0 14px;
  }
  img {
    display: block;
    max-width: 100%;
    height: auto;
    border-radius: 6px;
  }
  figcaption {
    margin-top: 4px;
    font-size: 12px;
    color: var(--muted);
  }
  .tbl {
    margin: 0 0 12px;
    overflow-x: auto;
  }
  table {
    border-collapse: collapse;
    font-size: 13px;
    min-width: 60%;
  }
  th,
  td {
    padding: 4px 10px;
    text-align: left;
    vertical-align: top;
    border-bottom: 1px solid var(--line);
    line-height: 1.45;
  }
  th {
    color: var(--muted);
    font-weight: 600;
    font-size: 12px;
    white-space: nowrap;
  }
  .xref {
    padding: 0;
    border: none;
    background: none;
    color: var(--accent);
    font: inherit;
    text-decoration: underline;
    text-underline-offset: 2px;
    cursor: pointer;
  }
  .xref:hover {
    color: var(--text);
  }
  .url {
    color: var(--muted);
    overflow-wrap: anywhere;
  }
</style>
