<script lang="ts">
  import type { SyncSettings } from "$lib/api/sync";
  import Button from "$lib/ui/Button.svelte";
  import { sync } from "./sync.svelte";

  // Every choice sync makes, with its default. The same fields are shown in the Sync view and in the
  // Settings view; a change is saved at once and drops the plan on screen, which was made without it.
  // "Sending side" is the library the tracks come from, "other side" the one they go to.
  $effect(() => {
    void sync.loadSettings();
  });
  const s = $derived(sync.settings);

  // the value is read here, at the event: the change itself is applied a moment later
  function set<K extends keyof SyncSettings>(key: K, value: SyncSettings[K]) {
    sync.update((x) => (x[key] = value));
  }
</script>

<div class="box">
  {#if sync.settingsWarning}<p class="warn" role="status">{sync.settingsWarning}</p>{/if}

  <label class="row">
    <span class="k">A track is on both sides</span>
    <select
      value={s.tracks_on_both}
      onchange={(e) => set("tracks_on_both", e.currentTarget.value as SyncSettings["tracks_on_both"])}
    >
      <option value="newer">Take the sending library's details when they are newer</option>
      <option value="source">Always take the sending library's details</option>
      <option value="leave">Leave both as they are</option>
    </select>
  </label>

  <label class="row">
    <span class="k">A mix has the same name</span>
    <select
      value={s.mixes_same_name}
      onchange={(e) => set("mixes_same_name", e.currentTarget.value as SyncSettings["mixes_same_name"])}
    >
      <option value="newer">Replace it when the sending library's is newer</option>
      <option value="replace">Always replace it</option>
      <option value="keep_both">Keep both: copy the new one with a different name</option>
      <option value="skip">Leave the one that is there</option>
    </select>
  </label>

  <label class="row">
    <span class="k">A case has the same name</span>
    <select
      value={s.cases_same_name}
      onchange={(e) => set("cases_same_name", e.currentTarget.value as SyncSettings["cases_same_name"])}
    >
      <option value="merge">Merge: keep its order, add the tracks it lacks</option>
      <option value="replace">Replace it with the sending library's</option>
      <option value="skip">Leave the one that is there</option>
    </select>
  </label>

  <label class="row">
    <span class="k">Play counts</span>
    <select
      value={s.play_counts}
      onchange={(e) => set("play_counts", e.currentTarget.value as SyncSettings["play_counts"])}
    >
      <option value="higher">Keep the higher count and the later date</option>
      <option value="leave">Leave them as they are</option>
    </select>
  </label>

  <label class="check">
    <input type="checkbox" checked={s.labels_follow} onchange={(e) => set("labels_follow", e.currentTarget.checked)} />
    Tags, colour labels and energy travel with the tracks that are copied
  </label>

  <label class="check danger">
    <input type="checkbox" checked={s.remove_missing} onchange={(e) => set("remove_missing", e.currentTarget.checked)} />
    List what the receiving library has and the sending library has not, so it can be removed
  </label>
  <p class="muted note">
    Off by default: nothing is ever removed. When it is on, those items are listed under Removals when you check, each named,
    and removed only if you leave them ticked and confirm the list.
  </p>

  <div><Button small variant="link" onclick={() => sync.resetSettings()}>Back to the defaults</Button></div>
</div>

<style>
  .box {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 10px;
    flex-wrap: wrap;
  }
  .k {
    width: 190px;
    flex: none;
    color: var(--muted);
  }
  select {
    min-width: 0;
    max-width: 100%;
  }
  .check {
    display: flex;
    align-items: baseline;
    gap: 6px;
  }
  .check input {
    margin: 0;
    accent-color: var(--accent);
  }
  .danger {
    color: var(--bad-text);
  }
  .note {
    margin: 0 0 0 22px;
    font-size: 12px;
  }
  .warn {
    margin: 0;
    color: var(--bad-text);
  }
</style>
