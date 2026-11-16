<script lang="ts">
  // The mix editor's View menu: how the timeline looks and follows, changed live and remembered.
  // The same choices are in Settings > Mixes > Mix editor view and Following the playhead (one copy
  // of each, so the two places always agree). Editing controls (Snap, Curves, Beat-align) stay in the toolbar.
  import { tick } from "svelte";
  import { sections } from "$lib/state/sections.svelte";
  import Button from "$lib/ui/Button.svelte";
  import { popoverPlace, type PopoverPlace } from "../tracks/popoverPlace";
  import type { FollowMode } from "./followLogic";
  import { followPrefs } from "./followPrefs.svelte";
  import { mv } from "./view.svelte";
  import { viewPrefs } from "./viewPrefs.svelte";
  import type { ClipHeightMode } from "./viewPrefsLogic";

  let open = $state(false);
  let place = $state<PopoverPlace | null>(null);
  let wrap = $state<HTMLDivElement | null>(null);
  let pop = $state<HTMLDivElement | null>(null);

  const v = $derived(viewPrefs.value);
  const HEIGHTS: { id: ClipHeightMode; label: string; tip: string }[] = [
    { id: "shrink", label: "Shrinks as you zoom out (default)", tip: "Zoomed in the clips are as tall as the window allows; zooming out makes them shorter, so the tools under them have more room." },
    { id: "full", label: "Always full height", tip: "The clips keep the height they have zoomed in, at every zoom." },
  ];
  const FOLLOW_MODES: { id: FollowMode; label: string }[] = [
    { id: "smooth", label: "Smoothly" },
    { id: "page", label: "A page at a time" },
  ];

  async function toggle(e: MouseEvent) {
    e.stopPropagation();
    if (open) return close(true);
    place = popoverPlace(e.currentTarget as HTMLElement, 330);
    open = true;
    await tick();
    pop?.querySelector<HTMLElement>("input")?.focus();
  }

  function close(refocus = false) {
    open = false;
    if (refocus) wrap?.querySelector("button")?.focus();
  }

  // Escape belongs to this menu alone while it is open (the editor would also clear its selection)
  $effect(() => {
    if (!open) return;
    const h = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      e.preventDefault();
      e.stopPropagation();
      close(true);
    };
    window.addEventListener("keydown", h, true);
    return () => window.removeEventListener("keydown", h, true);
  });

  /** Tabbing out of the menu closes it, so it never stays open behind the next control. */
  function focusout(e: FocusEvent) {
    const to = e.relatedTarget as Node | null;
    if (to && !pop?.contains(to) && !wrap?.querySelector("button")?.contains(to)) close();
  }
</script>

<svelte:window onclick={() => (open = false)} />

<div class="wrap" bind:this={wrap}>
  <Button small active={open} aria-haspopup="dialog" aria-expanded={open} onclick={toggle} title="How the timeline looks and follows the playhead: clip height, the whole-mix strip, sections, following">View ▾</Button>
  {#if open && place}
    <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
    <div
      class="pop"
      role="dialog"
      tabindex="-1"
      aria-label="View options"
      bind:this={pop}
      style:left="{place.left}px"
      style:top="{place.top}px"
      style:width="{place.width}px"
      style:max-height="{place.maxHeight}px"
      onclick={(e) => e.stopPropagation()}
      onfocusout={focusout}
    >
      <h4>Clip height</h4>
      <div role="radiogroup" aria-label="Clip height">
        {#each HEIGHTS as h (h.id)}
          <label class="check" title={h.tip}><input type="radio" name="view-clip-height" checked={v.clipHeight === h.id} onchange={() => viewPrefs.set({ clipHeight: h.id })} /> {h.label}</label>
        {/each}
      </div>

      <h4>Show</h4>
      <label class="check" title="A thin strip above the ruler that always shows the whole mix, with a box over the part shown below. Click it to centre the view, drag the box to scroll."
        ><input type="checkbox" checked={v.overview} onchange={(e) => viewPrefs.set({ overview: e.currentTarget.checked })} /> Whole-mix strip</label
      >
      <label class="check" title={sections.showMarkers ? "Draw the detected sections (intro, build, drop, breakdown, outro) on the clips. Clips of tracks without sections show nothing." : "Turned off in the Sections panel's settings (show markers on waveforms)"}>
        <input type="checkbox" checked={mv.sectionMarkers && sections.showMarkers} disabled={!sections.showMarkers} onchange={(e) => mv.setSectionMarkers(e.currentTarget.checked)} /> Sections on the clips
      </label>

      <h4>Following the playhead</h4>
      <label class="check" title="Scroll the timeline with the playhead while the mix plays (the same tick as in the transport)">
        <input type="checkbox" checked={v.follow} onchange={(e) => { mv.follow = e.currentTarget.checked; mv.resumeFollow(); }} /> Follow the playhead while playing
      </label>
      <div role="radiogroup" aria-label="How the view follows" class="modes">
        {#each FOLLOW_MODES as m (m.id)}
          <label class="check" class:off={!v.follow}><input type="radio" name="view-follow-mode" disabled={!v.follow} checked={followPrefs.value.mode === m.id} onchange={() => followPrefs.set({ mode: m.id })} /> {m.label}</label>
        {/each}
      </div>
      <p class="muted small">More choices, such as where the playhead sits, are in Settings &gt; Mixes. Snap, Curves and Beat-align stay in the toolbar: they change the mix, not the view.</p>
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
  .check {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 3px 0;
    font-size: 12px;
    cursor: pointer;
  }
  .check input {
    padding: 0;
  }
  .check.off {
    opacity: 0.5;
    cursor: default;
  }
  .modes {
    margin-left: 22px;
  }
  .muted {
    color: var(--muted);
  }
  .small {
    margin: 8px 0 0;
    font-size: 11px;
  }
</style>
