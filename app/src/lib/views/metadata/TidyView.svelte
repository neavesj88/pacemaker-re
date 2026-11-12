<script lang="ts">
  import { onMount, untrack } from "svelte";
  import { nav } from "$lib/state/nav.svelte";
  import { selection } from "$lib/state/selection.svelte";
  import { session } from "$lib/state/session.svelte";
  import Button from "$lib/ui/Button.svelte";
  import { plural } from "./cleanupLogic";
  import { cleanup } from "./cleanupState.svelte";
  import CleanupPreview from "./CleanupPreview.svelte";
  import CleanupRecipes from "./CleanupRecipes.svelte";
  import TagWritePanel from "./TagWritePanel.svelte";

  // Tidy metadata: recipes that fix titles, artists, genres and so on across many tracks, with a
  // preview of every change before anything is written, plus the optional write-back of the
  // library's tags into the audio files. Opened from the sidebar or from the track editor
  // (nav param "metatidy.ids").
  let tab = $state<"tidy" | "tags">("tidy");
  const lib = $derived(session.library);

  onMount(() => {
    void cleanup.load();
    void cleanup.loadCases();
    const ids = nav.take<number[]>("metatidy.ids");
    if (ids?.length) void cleanup.setScope({ kind: "ids", ids });
  });

  // saves the recipes and recomputes the preview a moment after anything it depends on changed
  $effect(() => {
    void cleanup.signature;
    untrack(() => cleanup.schedule());
  });

  const scopeKind = $derived(cleanup.scope.kind);
  const caseId = $derived(cleanup.scope.kind === "case" ? cleanup.scope.caseId : (cleanup.cases[0]?.id ?? 0));
  const nSelected = $derived(lib ? selection.trackIds.filter((id) => lib.tracks.some((t) => t.track_id === id)).length : 0);

  function pickKind(kind: string) {
    if (kind === "case") void cleanup.setScope({ kind: "case", caseId: cleanup.cases[0]?.id ?? 0 });
    else if (kind === "library") void cleanup.setScope({ kind: "library" });
    else void cleanup.setScope({ kind: "selection" });
  }
</script>

<div class="bar">
  <h2>Tidy metadata</h2>
  <Button small active={tab === "tidy"} onclick={() => (tab = "tidy")}>Clean-up recipes</Button>
  <Button small active={tab === "tags"} onclick={() => (tab = "tags")}>Write tags to files</Button>
</div>

{#if !lib}
  <p class="empty">Open a library to tidy its metadata.</p>
{:else}
  <div class="page">
    {#if cleanup.warning}<p class="warn" role="alert">{cleanup.warning}</p>{/if}
    {#if cleanup.error}<p class="warn" role="alert">{cleanup.error}</p>{/if}

    <fieldset class="scope">
      <legend>Tracks to look at</legend>
      <label>
        <input type="radio" name="scope" checked={scopeKind === "selection"} onchange={() => pickKind("selection")} />
        Highlighted in the Tracks view ({nSelected})
      </label>
      <label>
        <input type="radio" name="scope" checked={scopeKind === "case"} onchange={() => pickKind("case")} disabled={cleanup.cases.length === 0} />
        A case
      </label>
      <select
        aria-label="Case"
        disabled={scopeKind !== "case"}
        value={caseId}
        onchange={(e) => cleanup.setScope({ kind: "case", caseId: Number(e.currentTarget.value) })}
      >
        {#each cleanup.cases as c (c.id)}<option value={c.id}>{c.name}</option>{/each}
      </select>
      <label>
        <input type="radio" name="scope" checked={scopeKind === "library"} onchange={() => pickKind("library")} />
        The whole library ({lib.tracks.length})
      </label>
      {#if scopeKind === "ids"}
        <span class="chip">{plural(cleanup.ids.length, "track")} from the track editor</span>
      {/if}
      <span class="muted">{plural(cleanup.ids.length, "track")} in scope</span>
    </fieldset>

    {#if tab === "tidy"}
      <div class="split">
        <div class="left">
          <h3>Recipes and lists</h3>
          <CleanupRecipes />
        </div>
        <div class="right">
          <h3>Preview</h3>
          <CleanupPreview onTagsReady={() => (tab = "tags")} />
        </div>
      </div>
    {:else}
      <TagWritePanel />
    {/if}
  </div>
{/if}

<style>
  .bar {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 7px 12px;
    border-bottom: 1px solid var(--line);
  }
  h2 {
    margin: 0 8px 0 0;
    font-size: 15px;
  }
  h3 {
    margin: 0 0 8px;
    font-size: 13px;
  }
  .page {
    padding: 12px;
  }
  /* recipes on the left, the preview of what they do stays in view on the right */
  .split {
    display: grid;
    grid-template-columns: minmax(340px, 420px) minmax(0, 1fr);
    gap: 16px;
    align-items: start;
  }
  .right {
    position: sticky;
    top: 0;
    min-width: 0;
  }
  @media (max-width: 1000px) {
    .split {
      grid-template-columns: minmax(0, 1fr);
    }
    .right {
      position: static;
    }
  }
  .scope {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px 14px;
    border: 1px solid var(--line);
    border-radius: 6px;
    padding: 6px 12px 8px;
    margin: 0 0 10px;
  }
  .scope legend {
    color: var(--muted);
    font-size: 12px;
    padding: 0 4px;
  }
  .scope label {
    display: inline-flex;
    align-items: center;
    gap: 5px;
  }
  .scope select {
    padding: 2px 6px;
    max-width: 240px;
  }
  .scope input {
    padding: 0;
    accent-color: var(--accent);
  }
  .chip {
    font-size: 11px;
    padding: 1px 7px;
    border-radius: 9px;
    background: color-mix(in srgb, var(--green) 10%, transparent);
    color: var(--green);
  }
  .warn {
    color: var(--bad-text);
    background: var(--bad-bg);
    padding: 6px 10px;
    border-radius: 4px;
    margin: 0 0 10px;
  }
</style>
