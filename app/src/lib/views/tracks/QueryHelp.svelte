<script lang="ts">
  // The "?" next to the filters: what the search box understands. The rows are the ones in
  // query.ts (QUERY_HELP), whose examples the tests compile, so this cannot describe syntax that
  // does not work. "Try" puts an example into the search box.
  import { filters } from "$lib/state/filters.svelte";
  import { session } from "$lib/state/session.svelte";
  import Button from "$lib/ui/Button.svelte";
  import { popoverPlace, type PopoverPlace } from "./popoverPlace";
  import { QUERY_HELP } from "./query";

  let open = $state(false);
  let place = $state<PopoverPlace | null>(null);

  function toggle(e: MouseEvent) {
    e.stopPropagation();
    open = !open;
    if (open) place = popoverPlace(e.currentTarget as HTMLElement, 620);
  }
</script>

<svelte:window onclick={() => (open = false)} onkeydown={(e) => e.key === "Escape" && (open = false)} />

<div class="wrap">
  <Button small active={open} aria-expanded={open} aria-label="Search help" title="What the search box understands" onclick={toggle}>?</Button>
  {#if open && place}
    <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
    <div
      class="pop"
      role="dialog"
      tabindex="-1"
      aria-label="Search help"
      style:left="{place.left}px"
      style:top="{place.top}px"
      style:width="{place.width}px"
      style:max-height="{place.maxHeight}px"
      onclick={(e) => e.stopPropagation()}
    >
      <p class="lead">
        The search box above takes plain words and a small query language. Words are matched against the title, artist, album, genre,
        key, comments, label, remixer, composer, year and tags; spaces between terms mean “and”.
      </p>
      <table>
        <tbody>
          {#each QUERY_HELP as row (row.syntax)}
            <tr>
              <td class="syn"><code>{row.syntax}</code></td>
              <td>
                {row.meaning}
                <div class="ex">
                  <code>{row.example}</code>
                  <button type="button" class="try" onclick={() => (filters.query = row.example)} title="Put this example in the search box">Try</button>
                </div>
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
      <p class="foot muted">
        A word that looks like a field but is not one (<code>re:mix</code>) is searched as text and flagged under the filters; put it in quotes to
        search for it on purpose. A term the search cannot read is ignored, never an empty list. Tags, colours and energy are kept by this app only;
        they are not {session.isComputer ? "stored in the music library" : "on the device"}.
      </p>
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
    padding: 10px 12px;
    background: var(--panel);
    border: 1px solid var(--line-hi);
    border-radius: 6px;
  }
  .lead,
  .foot {
    margin: 0 0 8px;
    font-size: 12px;
    line-height: 1.45;
  }
  .foot {
    margin: 8px 0 0;
  }
  table {
    width: 100%;
    border-collapse: collapse;
    font-size: 12px;
  }
  td {
    padding: 5px 6px;
    vertical-align: top;
    border-top: 1px solid var(--line);
    line-height: 1.4;
  }
  .syn {
    white-space: nowrap;
    color: var(--green);
  }
  code {
    font-family: ui-monospace, "Cascadia Mono", Consolas, monospace;
  }
  .ex {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-top: 3px;
    color: var(--muted);
  }
  .try {
    padding: 0 6px;
    background: none;
    border: 1px solid var(--line);
    border-radius: 3px;
    color: var(--accent);
    font: inherit;
    font-size: 11px;
    cursor: pointer;
  }
  .try:hover {
    border-color: var(--accent);
  }
</style>
