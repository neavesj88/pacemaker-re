<script lang="ts">
  // The sections of one track as a row of chips under the grid editor's overview: kind, time,
  // bars and confidence; a click takes the waveform window there. Read-only.
  import { jobs } from "$lib/state/jobs.svelte";
  import { sections } from "$lib/state/sections.svelte";
  import Button from "$lib/ui/Button.svelte";
  import { clock, KIND_LABEL, kindColor, percent, phraseText } from "./sectionLogic";

  let { trackId, onseek }: { trackId: number | null; onseek: (secs: number) => void } = $props();

  const entry = $derived(trackId === null ? null : sections.entry(trackId));
</script>

{#if trackId !== null && sections.state}
  <div class="sections">
    {#if entry}
      <div class="head">
        <b>Sections</b>
        <span class="muted">estimated from the audio and this beat grid. {phraseText(entry)}</span>
        <Button small variant="ghost" onclick={() => sections.openPanel()}>Settings…</Button>
      </div>
      <div class="chips">
        {#each entry.sections as s, i (i)}
          <button class="chip" type="button" style:--c={kindColor(s.kind)} onclick={() => onseek(s.start_secs)} title="Go to {clock(s.start_secs)}">
            <i aria-hidden="true"></i>
            <b>{KIND_LABEL[s.kind]}</b>
            <span class="num">{clock(s.start_secs)}-{clock(s.end_secs)}</span>
            <span class="muted">bars {s.start_bar + 1}-{s.end_bar}, {percent(s.confidence)}</span>
          </button>
        {/each}
      </div>
    {:else}
      <div class="head">
        <b>Sections</b>
        <span class="muted">none for this track: not analysed yet, no first downbeat, too short, or no clear structure at this sensitivity.</span>
        <Button small onclick={() => sections.askAnalyse([trackId])} disabled={!jobs.canWrite}>Detect sections…</Button>
        <Button small variant="ghost" onclick={() => sections.openPanel()}>Settings…</Button>
      </div>
    {/if}
  </div>
{/if}

<style>
  .sections {
    margin-top: 8px;
    padding: 8px 10px;
    background: var(--panel);
    border: 1px solid var(--line);
    border-radius: 4px;
    font-size: 12px;
  }
  .head {
    display: flex;
    flex-wrap: wrap;
    gap: 4px 10px;
    align-items: center;
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-top: 6px;
  }
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 3px 8px;
    background: var(--deep);
    color: var(--text);
    border: 1px solid var(--line);
    border-left: 3px solid var(--c);
    border-radius: 4px;
    font: inherit;
    cursor: pointer;
  }
  .chip:hover {
    background: var(--hover);
  }
  .chip i {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--c);
  }
</style>
