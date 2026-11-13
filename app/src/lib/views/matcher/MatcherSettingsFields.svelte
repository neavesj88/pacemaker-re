<script lang="ts">
  // The matcher's saved choices (`matcher`, kept on this computer): shown under the request list
  // and in the Settings view. Changing one matches the list again when there is one.
  import { matcher } from "./matcher.svelte";
  import type { VersionMode } from "./matchLogic";

  const VERSION_HELP: Record<VersionMode, string> = {
    strict: "A different or missing mix name (Extended Mix, Radio Edit...) costs enough score that, with the default accept score, those matches are asked about.",
    lenient: "A different mix name costs a little score.",
    ignore: "Mix names are not compared.",
  };

  function number(e: Event) {
    return (e.currentTarget as HTMLInputElement).valueAsNumber;
  }
</script>

<div class="grid">
  <label>
    Plain lines read as
    <select value={matcher.lineOrder} onchange={(e) => matcher.setLineOrder(e.currentTarget.value === "title-artist" ? "title-artist" : "artist-title")}>
      <option value="artist-title">Artist - Title</option>
      <option value="title-artist">Title - Artist</option>
    </select>
  </label>
  <label>
    Accept without asking from score
    <input type="number" min="1" max="100" value={matcher.options.acceptAt} onchange={(e) => matcher.setOptions({ acceptAt: number(e) })} />
  </label>
  <label>
    Ask me from score
    <input type="number" min="1" max="100" value={matcher.options.reviewAt} onchange={(e) => matcher.setOptions({ reviewAt: number(e) })} />
  </label>
  <label>
    Two tracks closer than this are a tie
    <input type="number" min="0" max="50" value={matcher.options.margin} onchange={(e) => matcher.setOptions({ margin: number(e) })} />
  </label>
  <label>
    Different mix names
    <select value={matcher.options.versionMode} onchange={(e) => matcher.setOptions({ versionMode: e.currentTarget.value as VersionMode })}>
      <option value="strict">Count against the match</option>
      <option value="lenient">Count a little</option>
      <option value="ignore">Ignore</option>
    </select>
  </label>
  <label>
    Ask when the artist is less alike than (%)
    <input type="number" min="0" max="100" value={matcher.options.artistMin} onchange={(e) => matcher.setOptions({ artistMin: number(e) })} />
  </label>
  <label>
    Suggestions per line
    <input type="number" min="1" max="20" value={matcher.options.maxCandidates} onchange={(e) => matcher.setOptions({ maxCandidates: number(e) })} />
  </label>
</div>
<label class="inline">
  <input type="checkbox" checked={matcher.includeUndecided} onchange={(e) => matcher.setIncludeUndecided(e.currentTarget.checked)} />
  The missing list also holds the lines still waiting for your decision
</label>
<p class="muted">
  Score is 0-100. At or above the first number a single clear match counts as owned; between the two numbers you are asked; below the second it is missing.
  {VERSION_HELP[matcher.options.versionMode]} A line with no artist (or a library track with none) is always asked about unless the accept score is set to 80 or lower.
  These settings are remembered on this computer.
</p>
<label class="inline">
  <input type="checkbox" checked={matcher.includeUndecided} onchange={(e) => matcher.setIncludeUndecided(e.currentTarget.checked)} />
  The missing list also holds the lines still waiting for your decision
</label>
<p class="muted">
  Score is 0-100. At or above the first number a single clear match counts as owned; between the two numbers you are asked; below the second it is missing.
  {VERSION_HELP[matcher.options.versionMode]} A line with no artist (or a library track with none) is always asked about unless the accept score is set to 80 or lower.
  These settings are remembered on this computer.
</p>

<style>
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(230px, 1fr));
    gap: 8px 14px;
    margin: 8px 0;
  }
  .grid label {
    display: flex;
    flex-direction: column;
    gap: 3px;
    font-size: 12px;
    color: var(--muted);
  }
  .inline {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 12px;
    color: var(--muted);
  }
  .grid input,
  .grid select {
    background: var(--panel);
    color: var(--text);
    border: 1px solid var(--line);
    border-radius: 4px;
    padding: 4px 8px;
    font: inherit;
    font-size: 13px;
  }
</style>
