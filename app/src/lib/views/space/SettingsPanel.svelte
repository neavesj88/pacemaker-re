<script lang="ts">
  import { pickArchiveFolder } from "$lib/api/device";
  import { toast } from "$lib/state/toast.svelte";
  import Button from "$lib/ui/Button.svelte";
  import { space } from "./space.svelte";
  import { clampGb, clampInt, isOn, toggled } from "./spaceLogic";

  // Every threshold of the planner is here: the target, which tracks are nominated and why, which
  // are never suggested, and where archives go. Changes are saved as they are made and the list
  // is made again from them.
  let s = $derived(space.settings);
  let r = $derived(s.rules);

  async function chooseFolder() {
    try {
      const d = await pickArchiveFolder();
      if (d) space.update((x) => (x.archive_dir = d));
    } catch (e) {
      toast.show(String(e), "bad", 8000);
    }
  }
</script>

<div class="panel">
  <section>
    <h4>Target</h4>
    <label class="radio">
      <input type="radio" name="goal" checked={s.goal_kind === "free"} onchange={() => space.update((x) => (x.goal_kind = "free"))} />
      Keep
      <input
        class="n"
        type="number"
        min="0"
        max="100000"
        step="0.5"
        aria-label="Free space to keep, in GB"
        value={s.free_gb}
        onchange={(e) =>
          space.update((x) => {
            x.goal_kind = "free";
            x.free_gb = clampGb(e.currentTarget.value, 0, 100000, x.free_gb);
          })}
      />
      GB free
    </label>
    <label class="radio">
      <input type="radio" name="goal" checked={s.goal_kind === "card"} onchange={() => space.update((x) => (x.goal_kind = "card"))} />
      Fit a
      <input
        class="n"
        type="number"
        min="0.1"
        max="100000"
        step="1"
        list="card-sizes"
        aria-label="Size of the card to fit, in GB"
        value={s.card_gb}
        onchange={(e) =>
          space.update((x) => {
            x.goal_kind = "card";
            x.card_gb = clampGb(e.currentTarget.value, 0.1, 100000, x.card_gb);
          })}
      />
      GB card
    </label>
    <datalist id="card-sizes">
      {#each [16, 32, 64, 128, 256, 512, 1024] as n (n)}<option value={n}></option>{/each}
    </datalist>
    {#if s.goal_kind === "card"}
      <label class="line">
        keeping
        <input
          class="n"
          type="number"
          min="0"
          max="50"
          aria-label="Share of the card to keep free, in percent"
          value={s.reserve_percent}
          onchange={(e) => space.update((x) => (x.reserve_percent = clampInt(e.currentTarget.value, 0, 50, x.reserve_percent)))}
        />
        % of it free
      </label>
    {/if}
    <p class="hint muted">
      Free space uses the GB the Storage card shows. A card is counted as it is sold (1 GB = 1,000,000,000 bytes); the
      library is what it would have to hold: music, beat grids and the database.
    </p>
  </section>

  <section>
    <h4>Suggest tracks that</h4>
    <label class="line">
      <input type="checkbox" checked={r.never_played} onchange={(e) => space.update((x) => (x.rules.never_played = e.currentTarget.checked))} />
      were never played
    </label>
    <label class="line">
      <input
        type="checkbox"
        checked={isOn(r.stale_months)}
        onchange={(e) => space.update((x) => (x.rules.stale_months = toggled(x.rules.stale_months, e.currentTarget.checked, 6)))}
      />
      have not been played for
      <input
        class="n"
        type="number"
        min="1"
        max="240"
        aria-label="Months without a play"
        disabled={!isOn(r.stale_months)}
        value={r.stale_months || 6}
        onchange={(e) => space.update((x) => (x.rules.stale_months = clampInt(e.currentTarget.value, 1, 240, x.rules.stale_months)))}
      />
      months
    </label>
    <label class="line">
      <input
        type="checkbox"
        checked={isOn(r.low_rating_max)}
        onchange={(e) => space.update((x) => (x.rules.low_rating_max = toggled(x.rules.low_rating_max, e.currentTarget.checked, 2)))}
      />
      are rated up to
      <input
        class="n"
        type="number"
        min="1"
        max="5"
        aria-label="Highest star rating that counts as low"
        disabled={!isOn(r.low_rating_max)}
        value={r.low_rating_max || 2}
        onchange={(e) => space.update((x) => (x.rules.low_rating_max = clampInt(e.currentTarget.value, 1, 5, x.rules.low_rating_max)))}
      />
      stars
    </label>
    <label class="line sub">
      <input
        type="checkbox"
        disabled={!isOn(r.low_rating_max)}
        checked={r.unrated_is_low}
        onchange={(e) => space.update((x) => (x.rules.unrated_is_low = e.currentTarget.checked))}
      />
      count unrated tracks as low rated
    </label>
    <label class="line">
      <input type="checkbox" checked={r.duplicates} onchange={(e) => space.update((x) => (x.rules.duplicates = e.currentTarget.checked))} />
      repeat the audio of another track
    </label>
    <p class="hint muted">
      Duplicates are found by content, not by name. Of each group the copy in a mix, then in more cases, then rated and
      played most is kept. Looking for them reads files that share a size, which takes a moment on a big card.
    </p>
    <label class="line">
      A track needs at least
      <input
        class="n"
        type="number"
        min="1"
        max="4"
        aria-label="How many reasons a track needs"
        value={r.min_reasons}
        onchange={(e) => space.update((x) => (x.rules.min_reasons = clampInt(e.currentTarget.value, 1, 4, x.rules.min_reasons)))}
      />
      of these
    </label>
  </section>

  <section>
    <h4>Only tracks that are</h4>
    <label class="line">
      <input type="checkbox" checked={r.only_no_case} onchange={(e) => space.update((x) => (x.rules.only_no_case = e.currentTarget.checked))} />
      in no case
    </label>
    <label class="line">
      <input type="checkbox" checked={r.only_no_mix} onchange={(e) => space.update((x) => (x.rules.only_no_mix = e.currentTarget.checked))} />
      in no mix
    </label>
    <p class="hint muted">
      A track in a mix is never removed, whatever is set here; with this off it is listed so it can still be archived.
    </p>
  </section>

  <section>
    <h4>Never suggest</h4>
    <label class="line">
      <input type="checkbox" checked={r.protect_favourites} onchange={(e) => space.update((x) => (x.rules.protect_favourites = e.currentTarget.checked))} />
      tracks in Favourites
    </label>
    <label class="line">
      <input
        type="checkbox"
        checked={isOn(r.protect_rating_min)}
        onchange={(e) => space.update((x) => (x.rules.protect_rating_min = toggled(x.rules.protect_rating_min, e.currentTarget.checked, 4)))}
      />
      tracks rated at least
      <input
        class="n"
        type="number"
        min="1"
        max="5"
        aria-label="Lowest star rating that is protected"
        disabled={!isOn(r.protect_rating_min)}
        value={r.protect_rating_min || 4}
        onchange={(e) => space.update((x) => (x.rules.protect_rating_min = clampInt(e.currentTarget.value, 1, 5, x.rules.protect_rating_min)))}
      />
      stars
    </label>
    <label class="line">
      <input
        type="checkbox"
        checked={isOn(r.protect_new_days)}
        onchange={(e) => space.update((x) => (x.rules.protect_new_days = toggled(x.rules.protect_new_days, e.currentTarget.checked, 30)))}
      />
      tracks added in the last
      <input
        class="n"
        type="number"
        min="1"
        max="3650"
        aria-label="Days a new track is protected"
        disabled={!isOn(r.protect_new_days)}
        value={r.protect_new_days || 30}
        onchange={(e) => space.update((x) => (x.rules.protect_new_days = clampInt(e.currentTarget.value, 1, 3650, x.rules.protect_new_days)))}
      />
      days
    </label>
  </section>

  <section>
    <h4>Order</h4>
    <select
      aria-label="Order of the list"
      value={r.order}
      onchange={(e) => space.update((x) => (x.rules.order = e.currentTarget.value === "largest" ? "largest" : "certain"))}
    >
      <option value="certain">Clearest cases first</option>
      <option value="largest">Biggest files first (fewest tracks)</option>
    </select>
    <p class="hint muted">
      The ticks start with the first tracks in this order that reach the target.
    </p>
  </section>

  <section>
    <h4>Archive</h4>
    <div class="folder">
      <span class="path" title={s.archive_dir || undefined}>{s.archive_dir || "No folder chosen"}</span>
      <Button small onclick={chooseFolder}>Choose…</Button>
    </div>
    <label class="line">
      <input type="checkbox" checked={s.include_grids} onchange={(e) => space.update((x) => (x.include_grids = e.currentTarget.checked))} />
      include each track's beat grid (.str)
    </label>
    <label class="line">
      <input type="checkbox" checked={s.require_archive} onchange={(e) => space.update((x) => (x.require_archive = e.currentTarget.checked))} />
      archive before removing
    </label>
    <p class="hint muted">
      A track is removed only after its archive copy was written and read back correctly. With this off, "Remove"
      deletes from the card with no copy.
    </p>
  </section>

  {#if space.settingsWarning}<p class="warn">{space.settingsWarning}</p>{/if}
</div>

<style>
  .panel {
    display: flex;
    flex-direction: column;
    gap: 14px;
  }
  section {
    border-top: 1px solid var(--line);
    padding-top: 10px;
  }
  section:first-child {
    border-top: none;
    padding-top: 0;
  }
  h4 {
    margin: 0 0 6px;
  }
  .line,
  .radio {
    display: block;
    margin: 5px 0;
  }
  .line input,
  .radio input {
    vertical-align: middle;
  }
  .sub {
    margin-left: 22px;
    color: var(--muted);
  }
  .n {
    width: 68px;
    padding: 3px 6px;
    margin: 0 3px;
  }
  input[type="checkbox"],
  input[type="radio"] {
    padding: 0;
    margin: 0 6px 0 0;
    accent-color: var(--accent);
  }
  .hint {
    margin: 4px 0 0;
    font-size: 12px;
  }
  .folder {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-bottom: 6px;
  }
  .path {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-family: ui-monospace, Consolas, monospace;
    font-size: 12px;
    color: var(--muted);
  }
  .warn {
    color: var(--bad-text);
    margin: 0;
  }
  select {
    width: 100%;
  }
</style>
