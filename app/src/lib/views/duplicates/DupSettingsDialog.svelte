<script lang="ts">
  // Every choice of the duplicate finder in one place: how strict each way of finding copies is,
  // which copy is suggested for keeping, and what a merge does with case places, ratings and
  // plays. Saved in the app's own settings file, not on the Pacemaker.
  import type { DupSettings } from "$lib/api/duplicates";
  import { DEFAULT_SETTINGS, duplicates } from "$lib/state/duplicates.svelte";
  import Button from "$lib/ui/Button.svelte";
  import Modal from "$lib/ui/Modal.svelte";

  let draft = $state<DupSettings>({ ...duplicates.settings });

  const num = (e: Event & { currentTarget: HTMLInputElement }, min: number, max: number, fallback: number) => {
    const v = Math.round(Number(e.currentTarget.value));
    return Number.isFinite(v) ? Math.min(max, Math.max(min, v)) : fallback;
  };

  async function save() {
    await duplicates.saveSettings({ ...draft });
    duplicates.settingsOpen = false;
  }
</script>

<Modal title="Duplicate finder settings" onclose={() => (duplicates.settingsOpen = false)}>
  <div class="body">
    {#if duplicates.settingsWarning}<p class="warn" role="alert">{duplicates.settingsWarning}</p>{/if}

    <fieldset>
      <legend>Artist and title</legend>
      <label><input type="checkbox" bind:checked={draft.by_tags} /> Find copies with the same artist and title</label>
      <label><input type="checkbox" bind:checked={draft.strip_feat} /> Ignore “feat.” and “ft.” parts</label>
      <label><input type="checkbox" bind:checked={draft.ignore_brackets} /> Ignore text in brackets (otherwise “Original Mix” and “Extended Mix” are different tracks)</label>
      <label><input type="checkbox" bind:checked={draft.ignore_the} /> Ignore a leading “The” in the artist</label>
      <label class="fld">
        Lengths may differ by up to
        <input type="number" min="0" max="600" value={draft.tag_duration_secs} onchange={(e) => (draft.tag_duration_secs = num(e, 0, 600, 10))} /> seconds from the shortest of the set
        <span class="muted">(0 = do not compare lengths)</span>
      </label>
    </fieldset>

    <fieldset>
      <legend>File size and length</legend>
      <label><input type="checkbox" bind:checked={draft.by_size} /> Find copies with the same file size and length</label>
      <label class="fld">
        Lengths may differ by up to
        <input type="number" min="0" max="600" value={draft.size_duration_secs} onchange={(e) => (draft.size_duration_secs = num(e, 0, 600, 1))} /> seconds from the shortest of the set
      </label>
    </fieldset>

    <fieldset>
      <legend>Sound</legend>
      <label><input type="checkbox" bind:checked={draft.by_audio} /> Find copies by how they sound (reads every candidate file once; the result is cached)</label>
      <label class="fld">
        At least
        <input type="number" min="50" max="100" value={Math.round(draft.audio_threshold * 100)} onchange={(e) => (draft.audio_threshold = num(e, 50, 100, 90) / 100)} /> % alike
        <span class="muted">(higher finds fewer, surer copies)</span>
      </label>
      <label class="fld">
        At least
        <input type="number" min="50" max="100" value={Math.round(draft.audio_min_overlap * 100)} onchange={(e) => (draft.audio_min_overlap = num(e, 50, 100, 80) / 100)} /> % of the shorter track must line up with the other
        <span class="muted">(lower also matches a copy with a long intro or outro cut off)</span>
      </label>
      <label class="fld">
        Only compare tracks whose lengths differ by at most
        <input type="number" min="0" max="60" value={draft.audio_duration_secs} onchange={(e) => (draft.audio_duration_secs = num(e, 0, 60, 3))} /> seconds
      </label>
      <label class="fld">
        One copy may start up to
        <input type="number" min="0" max="30" value={draft.audio_offset_secs} onchange={(e) => (draft.audio_offset_secs = num(e, 0, 30, 5))} /> seconds after the other
      </label>
      <label class="fld">
        Tracks read at once
        <input type="number" min="0" max="8" value={draft.fingerprint_threads} onchange={(e) => (draft.fingerprint_threads = num(e, 0, 8, 0))} />
        <span class="muted">(0 = decide by the computer)</span>
      </label>
    </fieldset>

    <fieldset>
      <legend>Which copy to keep, and the merge</legend>
      <label class="fld">
        Suggest keeping the copy with the
        <select bind:value={draft.keeper_rule}>
          <option value="bitrate">highest bitrate</option>
          <option value="plays">most plays</option>
          <option value="newest">newest (added last)</option>
        </select>
        <span class="muted">(a copy whose file is missing is never suggested)</span>
      </label>
      <label><input type="checkbox" bind:checked={draft.move_cases} /> Move the other copies’ places in cases to the kept track</label>
      <label class="fld">
        Rating of the kept track:
        <select bind:value={draft.rating_rule}>
          <option value="max">the highest of the copies</option>
          <option value="keeper">its own</option>
        </select>
      </label>
      <label class="fld">
        Plays of the kept track:
        <select bind:value={draft.plays_rule}>
          <option value="max">the highest count of the copies</option>
          <option value="sum">the counts added up</option>
          <option value="keeper">its own</option>
        </select>
      </label>
      <label>
        <input type="checkbox" bind:checked={draft.allow_row_removal} />
        Allow removing the other copies from the library when merging
        <span class="muted">(off at first: deleting rows from <code>music.db</code> has not been tried on a real Pacemaker. The app copies <code>music.db</code> before the first write.)</span>
      </label>
      <p class="muted small">Whether to remove the copies, and whether to delete their files, is still asked for each merge; this setting only lets the first of the two be chosen.</p>
    </fieldset>
  </div>

  {#snippet actions()}
    <Button onclick={() => (draft = { ...DEFAULT_SETTINGS })}>Defaults</Button>
    <Button data-autofocus onclick={() => (duplicates.settingsOpen = false)}>Cancel</Button>
    <Button variant="primary" onclick={save}>Save</Button>
  {/snippet}
</Modal>

<style>
  .body {
    max-height: 66vh;
    overflow: auto;
    padding-right: 4px;
  }
  fieldset {
    margin: 0 0 10px;
    padding: 6px 12px 8px;
    border: 1px solid var(--line);
    border-radius: 6px;
  }
  legend {
    padding: 0 6px;
    color: var(--muted);
    font-weight: 600;
  }
  label {
    display: block;
    margin: 5px 0;
  }
  label.fld input[type="number"] {
    width: 64px;
  }
  .warn {
    color: var(--bad-text);
    margin: 0 0 8px;
  }
  .small {
    margin: 6px 0 0;
    font-size: 12px;
  }
</style>
