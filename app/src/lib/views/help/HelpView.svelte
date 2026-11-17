<script lang="ts">
  // The Help view: a search box, the articles grouped by topic (or the search results with a short
  // piece of text and the match marked), and the article reader. The articles are Markdown files in
  // ./articles; the index is built once (articles.ts) and searching only compares strings.
  import { tick } from "svelte";
  import { nav } from "$lib/state/nav.svelte";
  import { viewById } from "$lib/views/registry";
  import Button from "$lib/ui/Button.svelte";
  import ArticleBody from "./ArticleBody.svelte";
  import { helpIndex } from "./articles";
  import { help, openHelp } from "./help.svelte";
  import { groupByTopic, search, type Article } from "./helpLogic";

  const index = helpIndex();
  const groups = groupByTopic(index.entries.map((e) => e.article));
  /** Good places to begin, shown on the start page (ones that exist). */
  const STARTS = ["getting-started", "tracks-view", "search-syntax", "mixes-building", "device-page", "install-firmware", "pm-controls-tour", "troubleshooting"];
  const starts = STARTS.map((id) => index.byId.get(id)?.article).filter((a): a is Article => !!a);

  let searchEl = $state<HTMLInputElement | null>(null);
  let readerEl = $state<HTMLElement | null>(null);
  let cursor = $state(0);

  const query = $derived(help.query);
  const hits = $derived(search(index, query));
  const searching = $derived(query.trim() !== "");
  /** Every article the list shows, in order: what Up, Down and Enter walk through. */
  const visible = $derived<Article[]>(searching ? hits.map((h) => h.article) : groups.flatMap((g) => g.articles));
  const entry = $derived(help.articleId ? (index.byId.get(help.articleId) ?? null) : null);
  const outline = $derived(entry ? entry.blocks.filter((b) => b.t === "h" && b.level === 2) : []);
  const back = $derived(help.returnTo && help.returnTo !== "help" ? viewById(help.returnTo).label : "");

  // the cursor starts at the best hit whenever the query changes
  $effect(() => {
    void query;
    cursor = 0;
  });

  // focus on open, and again whenever something asks for the search box (Ctrl+/, F1 while open)
  $effect(() => {
    void help.focusTick;
    void tick().then(() => {
      searchEl?.focus();
      searchEl?.select();
    });
  });

  // Every openHelp(id) bumps `jump`: the reader goes to the heading it names, or to the top of a new article, and the
  // article's row in the list comes into view. Clicking the same heading twice therefore works twice. Coming back to Help
  // without a new jump (the sidebar, F1) puts the reader back where it was left.
  $effect(() => {
    const jump = help.jump;
    const id = help.articleId;
    const anchor = help.anchor;
    void tick().then(() => {
      if (!readerEl) return;
      if (jump === help.appliedJump) {
        readerEl.scrollTop = help.scrollTop;
        return;
      }
      help.appliedJump = jump;
      const target = id && anchor ? readerEl.querySelector<HTMLElement>(`#h-${CSS.escape(anchor)}`) : null;
      if (target) target.scrollIntoView({ block: "start" });
      else readerEl.scrollTop = 0;
      if (id) document.getElementById(`help-row-${id}`)?.scrollIntoView({ block: "nearest" });
    });
  });

  function openArticle(id: string, anchor: string | null = null) {
    openHelp(id, { anchor });
  }

  function onSearchKey(e: KeyboardEvent) {
    // the focus stays in the search box, so the page keys scroll the open article from here
    if ((e.key === "PageDown" || e.key === "PageUp") && readerEl && entry) {
      e.preventDefault();
      readerEl.scrollBy({ top: (e.key === "PageDown" ? 1 : -1) * readerEl.clientHeight * 0.9 });
    } else if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      if (visible.length === 0) return;
      e.preventDefault();
      cursor = Math.min(visible.length - 1, Math.max(0, cursor + (e.key === "ArrowDown" ? 1 : -1)));
      void tick().then(() => document.getElementById(`help-row-${visible[cursor]?.id}`)?.scrollIntoView({ block: "nearest" }));
    } else if (e.key === "Enter") {
      const a = visible[cursor];
      if (a) {
        e.preventDefault();
        openArticle(a.id, searching ? (hits[cursor]?.chapter?.slug ?? null) : null);
      }
    } else if (e.key === "Escape" && help.query) {
      e.preventDefault();
      help.query = "";
    }
  }

  /** Short windows start with the outline folded, so the article text gets the room. */
  const roomy = typeof window === "undefined" || window.innerHeight >= 720;

  const goBack = () => nav.go(help.returnTo && help.returnTo !== "help" ? help.returnTo : "tracks");
</script>

<div class="help">
  <div class="bar">
    <h2>Help</h2>
    <input
      bind:this={searchEl}
      class="find"
      type="search"
      placeholder="Search the help: battery, sd card, crossfader, bpm:…"
      aria-label="Search the help"
      autocomplete="off"
      spellcheck="false"
      bind:value={help.query}
      onkeydown={onSearchKey}
    />
    <span class="muted count" aria-live="polite">
      {#if searching}{hits.length} {hits.length === 1 ? "article" : "articles"}{:else}{index.entries.length} articles{/if}
    </span>
    <span class="grow"></span>
    <span class="muted keys">F1 opens Help · Ctrl+/ searches it</span>
    {#if back}<Button small onclick={goBack} title="Go back to where you were">← Back to {back}</Button>{/if}
  </div>

  <div class="cols">
    <nav class="list" aria-label="Help articles">
      {#if searching}
        {#if hits.length === 0}
          <p class="none">No article matches “{query.trim()}”. Try fewer or different words, or pick a topic below.</p>
        {:else}
          {#if hits[0].partial}<p class="none">No article has all of those words. These have some of them.</p>{/if}
          <ul class="results">
            {#each hits as h, i (h.article.id)}
              <li>
                <button
                  type="button"
                  id="help-row-{h.article.id}"
                  class="row"
                  class:on={help.articleId === h.article.id}
                  class:cur={cursor === i}
                  onclick={() => openArticle(h.article.id, h.chapter?.slug ?? null)}
                >
                  <span class="t">{#each h.title as s, si (si)}{#if s.hit}<mark>{s.text}</mark>{:else}{s.text}{/if}{/each}</span>
                  <span class="topic">{h.article.topic}</span>
                  {#if h.chapter}
                    <span class="chap">› {#each h.chapter.text as s, si (si)}{#if s.hit}<mark>{s.text}</mark>{:else}{s.text}{/if}{/each}</span>
                  {/if}
                  {#if h.snippet}
                    <span class="snip">{#each h.snippet as s, si (si)}{#if s.hit}<mark>{s.text}</mark>{:else}{s.text}{/if}{/each}</span>
                  {:else}
                    <span class="snip">{h.article.summary}</span>
                  {/if}
                </button>
              </li>
            {/each}
          </ul>
        {/if}
      {/if}
      {#if !searching || hits.length === 0}
        {#each groups as g (g.topic)}
          <section>
            <h3>{g.topic}</h3>
            <ul>
              {#each g.articles as a (a.id)}
                <li>
                  <button type="button" id="help-row-{a.id}" class="row slim" class:on={help.articleId === a.id} onclick={() => openArticle(a.id)} title={a.summary}>{a.title}</button>
                </li>
              {/each}
            </ul>
          </section>
        {/each}
      {/if}
    </nav>

    <!-- focusable so the keyboard can scroll it (PageUp, PageDown, arrows) once a link or Tab has put the focus here -->
    <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
    <article class="reader" bind:this={readerEl} tabindex="0" aria-label={entry ? entry.article.title : "Help"} onscroll={() => (help.scrollTop = readerEl?.scrollTop ?? 0)}>
      {#if entry}
        <div class="crumb muted">{entry.article.topic}</div>
        <h1>{entry.article.title}</h1>
        <p class="lead muted">{entry.article.summary}</p>
        {#if outline.length >= 3}
          <details class="outline" open={roomy}>
            <summary class="muted">In this article</summary>
            <p>{#each outline as o, oi (oi)}{#if o.t === "h"}<button type="button" class="xref" onclick={() => openArticle(entry.article.id, o.slug)}>{o.text}</button>{/if}{/each}</p>
          </details>
        {/if}
        <ArticleBody blocks={entry.blocks} known={(id) => index.byId.has(id)} onhelp={openArticle} />
      {:else}
        <h1>How can we help?</h1>
        <p class="lead muted">
          Search above, or pick an article from the list. The search looks in titles, keywords, headings and the text, and works without a connection.
        </p>
        <h2>Good places to start</h2>
        <ul class="starts">
          {#each starts as a (a.id)}
            <li><button type="button" class="xref" onclick={() => openArticle(a.id)}>{a.title}</button> <span class="muted">{a.summary}</span></li>
          {/each}
        </ul>
        <p class="muted small">Shortcuts: F1 opens Help from anywhere. Ctrl+/ opens it with the cursor in the search box. Esc clears the search.</p>
      {/if}
    </article>
  </div>
</div>

<style>
  .help {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
  }
  .bar {
    flex: none;
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 7px 12px;
    border-bottom: 1px solid var(--line);
  }
  h2 {
    margin: 0 8px 0 0;
    font-size: 15px;
  }
  .find {
    width: 420px;
    max-width: 100%;
    flex: 0 1 420px;
  }
  .count,
  .keys {
    font-size: 12px;
    white-space: nowrap;
  }
  .grow {
    flex: 1;
  }
  .cols {
    flex: 1;
    display: flex;
    min-height: 0;
  }
  .list {
    flex: none;
    width: 340px;
    overflow: auto;
    padding: 8px 6px 16px;
    background: var(--panel);
    border-right: 1px solid var(--line);
  }
  .list h3 {
    margin: 12px 8px 3px;
    font-size: 11px;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--muted);
    font-weight: 400;
  }
  .list ul {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .row {
    display: block;
    width: 100%;
    box-sizing: border-box;
    padding: 6px 8px;
    border: none;
    border-radius: 4px;
    background: none;
    color: var(--text);
    font: inherit;
    text-align: left;
    cursor: pointer;
  }
  .row.slim {
    padding: 4px 8px;
  }
  .row:hover {
    background: var(--hover);
  }
  .row.cur {
    outline: 1px solid var(--line-hi);
  }
  .row.on {
    background: var(--accent-soft);
    color: var(--accent);
  }
  .results .row {
    padding: 8px 8px 9px;
  }
  .t {
    display: block;
    font-weight: 600;
  }
  .topic {
    display: block;
    font-size: 11px;
    color: var(--muted);
    margin: 1px 0 2px;
  }
  .chap {
    display: block;
    font-size: 12px;
    font-weight: 600;
    margin: 0 0 2px;
  }
  .snip {
    display: block;
    font-size: 12px;
    line-height: 1.4;
    color: var(--muted);
    overflow-wrap: anywhere;
  }
  .row.on .snip {
    color: var(--text);
  }
  mark {
    background: color-mix(in srgb, var(--green) 28%, transparent);
    color: var(--text);
    border-radius: 2px;
    padding: 0 1px;
  }
  .none {
    margin: 8px;
    color: var(--muted);
  }
  .reader {
    flex: 1;
    min-width: 0;
    overflow: auto;
    padding: 14px 28px 40px;
    box-sizing: border-box;
  }
  .reader:focus-visible {
    outline: 1px solid var(--line-hi);
    outline-offset: -2px;
  }
  /* a comfortable line length, but wider on a big screen so the text does not hug the left edge */
  .reader > :global(*) {
    max-width: clamp(780px, 55vw, 1100px);
  }
  .crumb {
    font-size: 12px;
  }
  h1 {
    margin: 2px 0 6px;
    font-size: 22px;
  }
  .lead {
    margin: 0 0 12px;
    line-height: 1.5;
  }
  .outline {
    margin: 0 0 8px;
    font-size: 12px;
  }
  .outline summary {
    cursor: pointer;
  }
  .outline p {
    display: flex;
    flex-wrap: wrap;
    gap: 4px 14px;
    margin: 4px 0 0;
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
  .starts {
    margin: 0 0 14px;
    padding-left: 18px;
    line-height: 1.7;
  }
  .small {
    font-size: 12px;
  }
  @media (max-width: 1150px) {
    .list {
      width: 280px;
    }
    .keys {
      display: none;
    }
    .reader {
      padding: 12px 18px 40px;
    }
  }
</style>
