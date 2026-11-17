<script lang="ts">
  import { FEAT_FORMS, type CleanupSettings } from "$lib/api/metacleanup";
  import { TEXT_FIELDS } from "$lib/api/metadata";
  import Button from "$lib/ui/Button.svelte";
  import Modal from "$lib/ui/Modal.svelte";
  import { cleanup } from "./cleanupState.svelte";
  import { FIELD_LABELS } from "./draft";
  import TextListField from "./TextListField.svelte";

  // The recipes the user ticks, and the lists and maps behind them. Everything here is a saved
  // setting (app config dir): the tool starts the next time exactly as it was left.
  let s = $derived(cleanup.settings);

  // "Reset to defaults" replaces hand-made lists and maps, so it asks first and names each one.
  let confirmReset = $state(false);
  const replaced = $derived(
    s
      ? [
          ["Genre map", s.genre_map.length],
          ["Artist map", s.artist_map.length],
          ["Always upper case words", s.keep_upper.length],
          ["Small words (Title Case)", s.small_words.length],
          ["Artist separators", s.artist_separators.length],
          ["Promo phrases", s.promo_phrases.length],
          ["Remix words", s.remix_words.length],
          ["Not a person words", s.remixer_ignore.length],
        ] as [string, number][]
      : [],
  );

  function toggle(list: string[], f: string, on: boolean): string[] {
    const without = list.filter((x) => x !== f);
    return on ? TEXT_FIELDS.filter((k) => k === f || without.includes(k)) : without;
  }
</script>

{#snippet fields(key: "case_fields" | "promo_fields", on: boolean)}
  <div class="fields" role="group" aria-label="Fields">
    {#each TEXT_FIELDS as f (f)}
      <label class:off={!on}>
        <input
          type="checkbox"
          disabled={!on}
          checked={s![key].includes(f)}
          onchange={(e) => (s![key] = toggle(s![key], f, e.currentTarget.checked))}
        />
        {FIELD_LABELS[f]}
      </label>
    {/each}
  </div>
{/snippet}

{#if s}
  <div class="grid">
    <section class="card">
      <label class="head"><input type="checkbox" bind:checked={s.trim} /> Trim and collapse white space</label>
      <p class="muted">
        Removes extra spaces in every text field. Comments keep line breaks.
      </p>
    </section>

    <section class="card">
      <label class="head"><input type="checkbox" bind:checked={s.strip_number} /> Strip leading track numbers</label>
      <p class="muted">“01 - Title” and “1) Title” become “Title”.</p>
      <label class="opt" class:off={!s.strip_number}>
        A track number has up to
        <input
          class="num"
          type="number"
          min="1"
          max="6"
          step="1"
          disabled={!s.strip_number}
          value={s.number_max_digits}
          oninput={(e) => {
            const n = e.currentTarget.valueAsNumber;
            if (Number.isFinite(n)) s!.number_max_digits = Math.min(6, Math.max(1, Math.trunc(n)));
          }}
        />
        digits
      </label>
      <label class="opt" class:off={!s.strip_number}>
        <input type="checkbox" disabled={!s.strip_number} bind:checked={s.number_space_only} />
        Also “01 Title” (number and space only)
      </label>
    </section>

    <section class="card">
      <label class="head"><input type="checkbox" bind:checked={s.strip_promo} /> Strip URLs and promo text</label>
      <p class="muted">
        Removes web addresses and bracketed promo such as “(Free Download)”.
      </p>
      {@render fields("promo_fields", s.strip_promo)}
      <div class:off={!s.strip_promo}>
        <TextListField
          kind="list"
          label="Promo phrases (one per line)"
          hint="Brackets with one of these phrases are removed."
          value={s.promo_phrases}
          onchange={(v) => (s!.promo_phrases = v)}
          rows={5}
        />
      </div>
    </section>

    <section class="card">
      <label class="head"><input type="checkbox" bind:checked={s.feat_normalise} /> One form of “feat.”</label>
      <p class="muted">Writes feat., ft. and featuring the same way.</p>
      <label class="opt" class:off={!s.feat_normalise}>
        Write it as
        <select disabled={!s.feat_normalise} bind:value={s.feat_form}>
          {#each FEAT_FORMS as f (f)}<option value={f}>{f}</option>{/each}
        </select>
      </label>
      <label class="head sub"><input type="checkbox" bind:checked={s.feat_to_artist} /> Move “feat. X” from the title to the artist</label>
      <p class="muted">“Title (feat. X)” becomes “Title” by “Artist feat. X”.</p>
    </section>

    <section class="card">
      <label class="head"><input type="checkbox" bind:checked={s.extract_remixer} /> Take the remixer from “(X Remix)”</label>
      <p class="muted">Copies X to the Remixer field.</p>
      <label class="opt" class:off={!s.extract_remixer}>
        <input type="checkbox" disabled={!s.extract_remixer} bind:checked={s.overwrite_remixer} />
        Replace a remixer that is already filled in
      </label>
      <div class="two" class:off={!s.extract_remixer}>
        <TextListField kind="list" label="Remix words" hint="A bracket ending in one of these is a remix credit." value={s.remix_words} onchange={(v) => (s!.remix_words = v)} rows={4} />
        <TextListField kind="list" label="Not a person" hint="“Radio Edit” and “Extended Mix” name no remixer." value={s.remixer_ignore} onchange={(v) => (s!.remixer_ignore = v)} rows={4} />
      </div>
    </section>

    <section class="card">
      <label class="head"><input type="checkbox" bind:checked={s.title_case} /> Title Case</label>
      <p class="muted">Capitalises each word; small words stay lower case.</p>
      {@render fields("case_fields", s.title_case)}
      <div class:off={!s.title_case}>
        <TextListField
          kind="list"
          label="Always upper case (one per line)"
          hint="Always written like this."
          value={s.keep_upper}
          onchange={(v) => (s!.keep_upper = v)}
          rows={4}
        />
        <TextListField
          kind="list"
          label="Small words (one per line)"
          hint="Lower case inside a title."
          value={s.small_words}
          onchange={(v) => (s!.small_words = v)}
          rows={5}
        />
      </div>
    </section>

    <section class="card">
      <label class="head"><input type="checkbox" bind:checked={s.genre_map_on} /> Genre names</label>
      <p class="muted">Replaces a genre matching the left side with the right.</p>
      <div class:off={!s.genre_map_on}>
        <TextListField kind="map" label="Genre map (from -> to)" hint="One per line, for example “Old name -> New name”." value={s.genre_map} onchange={(v) => (s!.genre_map = v)} rows={6} />
      </div>
    </section>

    <section class="card">
      <label class="head"><input type="checkbox" bind:checked={s.artist_map_on} /> Artist names</label>
      <p class="muted">Replaces an artist name matching the left side.</p>
      <div class:off={!s.artist_map_on}>
        <TextListField kind="map" label="Artist map (from -> to)" hint="One per line. Empty to start with." value={s.artist_map} onchange={(v) => (s!.artist_map = v)} rows={6} />
        <TextListField
          kind="list"
          label="Separators between names (one per line)"
          hint="Splits “A & B”, “A, B”, “A feat. B”."
          value={s.artist_separators}
          onchange={(v) => (s!.artist_separators = v)}
          rows={4}
        />
      </div>
    </section>

    <section class="card wide">
      <label class="head"><input type="checkbox" bind:checked={s.write_back} /> Then offer to write the changes into the audio files’ own tags</label>
      <p class="muted">
        Lists tag changes on the “Write tags to files” page. Nothing is written until you confirm.
      </p>
    </section>
  </div>

  <div class="foot">
    <Button small onclick={() => (confirmReset = true)} title="Restore the built-in recipes, lists and maps">Reset to defaults</Button>
    <span class="muted" role="status">
      {#if cleanup.saved === "saving"}Saving…{:else if cleanup.saved === "saved"}Saved on this computer{:else if cleanup.saved === "error"}Could not save: {cleanup.saveError}{:else}Saved automatically on this computer{/if}
    </span>
  </div>
{/if}

{#if confirmReset}
  <Modal title="Reset the clean-up settings?" onclose={() => (confirmReset = false)}>
    <p class="warn">
      Replaces your recipe settings, lists and maps with the built-in ones. This cannot be undone:
    </p>
    <ul class="gone">
      {#each replaced as [name, n] (name)}
        <li>{name}: {n === 0 ? "empty now" : n === 1 ? "1 entry now" : `${n} entries now`}, replaced by the built-in list</li>
      {/each}
      <li>Tick boxes, “feat.” form, fields and digits go back to defaults</li>
    </ul>
    {#snippet actions()}
      <Button data-autofocus onclick={() => (confirmReset = false)}>Keep my settings</Button>
      <Button
        onclick={() => {
          confirmReset = false;
          void cleanup.resetToDefaults();
        }}
      >Replace them with the defaults</Button>
    {/snippet}
  </Modal>
{/if}

<style>
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(340px, 1fr));
    gap: 10px;
    align-items: start;
  }
  .card {
    background: var(--panel);
    border: 1px solid var(--line);
    border-radius: 6px;
    padding: 10px 12px;
    display: flex;
    flex-direction: column;
    gap: 6px;
    min-width: 0;
  }
  .card.wide {
    grid-column: 1 / -1;
  }
  .card p {
    margin: 0;
    font-size: 12px;
  }
  .head {
    display: flex;
    align-items: center;
    gap: 8px;
    font-weight: 600;
  }
  .head.sub {
    margin-top: 6px;
  }
  .opt {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 12px;
  }
  .opt select {
    padding: 2px 6px;
  }
  .num {
    width: 3.5em;
    padding: 2px 4px;
  }
  .warn {
    margin: 0 0 8px;
  }
  .gone {
    margin: 0 0 8px;
    padding: 6px 6px 6px 22px;
    font-size: 12px;
    max-height: 240px;
    overflow: auto;
    background: var(--deep);
    border: 1px solid var(--line);
    border-radius: 4px;
  }
  input[type="checkbox"] {
    padding: 0;
    accent-color: var(--accent);
  }
  .fields {
    display: flex;
    flex-wrap: wrap;
    gap: 2px 12px;
    font-size: 12px;
  }
  .fields label {
    display: inline-flex;
    align-items: center;
    gap: 4px;
  }
  .two {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 8px;
  }
  /* a recipe that is switched off keeps showing its settings, dimmed */
  .off {
    opacity: 0.55;
  }
  .foot {
    display: flex;
    align-items: center;
    gap: 12px;
    margin-top: 10px;
    font-size: 12px;
  }
</style>
