<script lang="ts">
  import { session } from "$lib/state/session.svelte";
  // Saved filter presets and the two view settings. A preset is the search text plus the
  // toolbar filters (and, if asked, the sort); applying one replaces what is set now. Presets
  // live in this app's own store for the library (commands/tracks/views.rs): they are not on the
  // device and do not appear on the Pacemaker.
  import { toast } from "$lib/state/toast.svelte";
  import Button from "$lib/ui/Button.svelte";
  import { COLUMNS } from "./columns";
  import { popoverPlace, type PopoverPlace } from "./popoverPlace";
  import { tracksState } from "./tracksState.svelte";
  import { describeViewState, viewStateIsClear } from "./viewState";
  import type { SortSpec } from "./trackLogic";

  let open = $state(false);
  let place = $state<PopoverPlace | null>(null);
  let name = $state("");
  let withSort = $state(false);
  /** The name of the existing preset a save would replace, once the user is asked. */
  let replaceAsk = $state<string | null>(null);
  /** The id of the preset waiting for its delete to be confirmed. */
  let deleteAsk = $state<string | null>(null);
  let busy = $state(false);

  const sortLabel = (k: SortSpec) => COLUMNS.find((c) => c.sort === k.field)?.label || (k.field === "fav" ? "Favourite" : k.field);
  const caseName = (id: number) => session.library?.cases.find((c) => c.id === id)?.name;
  const describe = (s: Parameters<typeof describeViewState>[0]) => describeViewState(s, sortLabel, caseName);

  // what a save would store: the same capture the save itself takes
  const current = $derived(tracksState.captureState(withSort));
  const nothingToSave = $derived(viewStateIsClear(current) && !withSort);
  const clean = $derived(name.trim().split(/\s+/).filter(Boolean).join(" "));
  const taken = $derived(tracksState.presets.find((p) => p.name.toLowerCase() === clean.toLowerCase()));

  $effect(() => {
    if (!open) {
      replaceAsk = null;
      deleteAsk = null;
    }
  });
  $effect(() => {
    // changing the name withdraws a pending "replace?" question about the old one
    clean;
    replaceAsk = null;
  });

  async function save(replace: boolean) {
    if (!clean || busy) return;
    if (taken && !replace) {
      replaceAsk = taken.name;
      return;
    }
    busy = true;
    const ok = await tracksState.savePreset(clean, withSort, replace);
    busy = false;
    if (ok) {
      toast.show(`Saved the preset “${clean}”${replace ? " (replaced the old one)" : ""}`, "ok");
      name = "";
      replaceAsk = null;
    }
  }

  function toggle(e: MouseEvent) {
    e.stopPropagation();
    open = !open;
    if (open) place = popoverPlace(e.currentTarget as HTMLElement, 400);
  }

  function apply(p: (typeof tracksState.presets)[number]) {
    if (!tracksState.applyPreset(p)) return;
    open = false;
    toast.show(`Applied “${p.name}”`, "ok");
  }

  async function remove(id: string, label: string) {
    busy = true;
    const ok = await tracksState.deletePreset(id);
    busy = false;
    deleteAsk = null;
    if (ok) toast.show(`Deleted the preset “${label}”`, "ok");
  }
</script>

<svelte:window onclick={() => (open = false)} onkeydown={(e) => e.key === "Escape" && (open = false)} />

<div class="wrap">
  <Button small active={open} aria-expanded={open} onclick={toggle} title="Saved filters, and what the table remembers">
    Presets{tracksState.presets.length ? ` (${tracksState.presets.length})` : ""}
  </Button>
  {#if open && place}
    <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
    <div
      class="pop"
      role="dialog"
      tabindex="-1"
      aria-label="Filter presets"
      style:left="{place.left}px"
      style:top="{place.top}px"
      style:width="{place.width}px"
      style:max-height="{place.maxHeight}px"
      onclick={(e) => e.stopPropagation()}
    >
      {#if tracksState.viewsNotice}<p class="notice">{tracksState.viewsNotice}</p>{/if}

      <h4>Saved filters</h4>
      {#each tracksState.presets as p (p.id)}
        <div class="preset">
          {#if deleteAsk === p.id}
            <span class="ask">Delete the preset “{p.name}”? This cannot be undone.</span>
            <span class="acts">
              <Button small variant="primary" disabled={busy} onclick={() => remove(p.id, p.name)}>Delete “{p.name}”</Button>
              <Button small onclick={() => (deleteAsk = null)}>Keep it</Button>
            </span>
          {:else}
            <button type="button" class="apply" title="Replace the current search and filters with this preset" onclick={() => apply(p)}>
              <span class="pname">{p.name}</span>
              <span class="pdesc muted">{describe(tracksState.presetState(p))}</span>
            </button>
            <Button small variant="ghost" aria-label="Delete the preset {p.name}" title="Delete this preset" onclick={() => (deleteAsk = p.id)}>Delete</Button>
          {/if}
        </div>
      {:else}
        <p class="muted none">No saved filters for this library yet.</p>
      {/each}

      <h4>Save the current view</h4>
      <p class="muted what">{nothingToSave ? "Nothing is searched or filtered, and the sort is not included." : `Saves: ${describe(current)}.`}</p>
      <form
        onsubmit={(e) => {
          e.preventDefault();
          void save(false);
        }}
      >
        <input placeholder="Preset name" maxlength="60" aria-label="Preset name" bind:value={name} />
        <label class="check"><input type="checkbox" bind:checked={withSort} /> Include the sort order</label>
        {#if replaceAsk}
          <p class="ask">A preset called “{replaceAsk}” already exists. Replace it with the view above?</p>
          <span class="acts">
            <Button small variant="primary" type="button" disabled={busy} onclick={() => save(true)}>Replace “{replaceAsk}”</Button>
            <Button small type="button" onclick={() => (replaceAsk = null)}>Cancel</Button>
          </span>
        {:else}
          <Button small variant="primary" type="submit" disabled={!clean || busy || nothingToSave}>{taken ? `Replace “${taken.name}”…` : "Save preset"}</Button>
        {/if}
      </form>

      <h4>Remembering</h4>
      <label class="check">
        <input type="checkbox" checked={tracksState.prefs.rememberView} onchange={(e) => tracksState.setRememberView(e.currentTarget.checked)} />
        Remember each library’s search, filters and sort
      </label>
      <label class="check">
        <input type="checkbox" checked={tracksState.prefs.findClearsFilters} onchange={(e) => tracksState.setFindClearsFilters(e.currentTarget.checked)} />
        “Find” from History clears the filters that hide the track
      </label>
      <p class="muted small">Presets and the remembered view are kept by this app only; they are not {session.isComputer ? "stored in the music library" : "on the device"}.</p>
    </div>
  {/if}
</div>

<style>
  .wrap {
    position: relative;
  }
  .pop {
    position: fixed;
    z-index: 40;
    box-sizing: border-box;
    overflow-y: auto;
    padding: 8px 12px 10px;
    background: var(--panel);
    border: 1px solid var(--line-hi);
    border-radius: 6px;
  }
  h4 {
    margin: 10px 0 4px;
    font-size: 11px;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--muted);
  }
  h4:first-child {
    margin-top: 2px;
  }
  .notice {
    margin: 0 0 6px;
    padding: 6px 8px;
    border-radius: 4px;
    background: var(--bad-bg);
    color: var(--bad-text);
    font-size: 12px;
  }
  .preset {
    display: flex;
    align-items: center;
    gap: 6px;
    border-top: 1px solid var(--line);
    padding: 3px 0;
  }
  .apply {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 1px;
    padding: 3px 4px;
    background: none;
    border: 0;
    color: inherit;
    font: inherit;
    text-align: left;
    cursor: pointer;
  }
  .apply:hover,
  .apply:focus-visible {
    background: var(--hover);
  }
  .pname {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .pdesc {
    font-size: 11px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .none,
  .what {
    margin: 2px 0 6px;
    font-size: 12px;
  }
  .ask {
    margin: 4px 0;
    font-size: 12px;
    color: var(--text);
  }
  .acts {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  form {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 6px;
  }
  form input:not([type]) {
    box-sizing: border-box;
    width: 100%;
  }
  .check {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 2px 0;
    font-size: 12px;
    cursor: pointer;
  }
  .check input {
    padding: 0;
  }
  .small {
    margin: 6px 0 0;
    font-size: 11px;
  }
</style>
