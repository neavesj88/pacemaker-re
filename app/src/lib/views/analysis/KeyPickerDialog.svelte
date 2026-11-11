<script lang="ts">
  // Sets one key on the chosen tracks by hand: pick it on the Camelot wheel layout (each button
  // shows the Camelot code and the musical name) or type it. Every track affected is listed with
  // the key it has now. The key is stored as the musical name, the way the detector writes it.
  import { analysis } from "$lib/state/analysis.svelte";
  import { canonicalKeyName, PICKER_KEYS, pickerKeyOf, spellings, trackLabel } from "$lib/state/keyLogic";
  import Button from "$lib/ui/Button.svelte";
  import Modal from "$lib/ui/Modal.svelte";
  import { keyColour } from "$lib/views/tracks/trackLogic";

  let { ids }: { ids: number[] } = $props();

  const tracks = $derived(analysis.tracksOf(ids));
  let choice = $state<string | null>(null);
  let typed = $state("");
  let saving = $state(false);

  const chosen = $derived(choice ? (PICKER_KEYS.find((k) => k.name === choice) ?? null) : null);
  const typedBad = $derived(typed.trim() !== "" && canonicalKeyName(typed) === null);
  const minor = PICKER_KEYS.filter((k) => k.minor);
  const major = PICKER_KEYS.filter((k) => !k.minor);

  function pick(name: string) {
    choice = name;
    typed = "";
  }

  function onType(text: string) {
    typed = text;
    choice = canonicalKeyName(text);
  }

  async function apply() {
    if (!choice || saving) return;
    saving = true;
    try {
      if (await analysis.setKey(ids, choice)) analysis.closeDialog();
    } finally {
      saving = false;
    }
  }

  const label = (key: string) => {
    const k = pickerKeyOf(key);
    return k ? `${key} (${k.camelot})` : key.trim() || "no key";
  };
</script>

<Modal title={tracks.length === 1 ? "Set the key" : `Set the key of ${tracks.length} tracks`} onclose={() => !saving && analysis.closeDialog()}>
  {#each [{ name: "Minor (A)", keys: minor }, { name: "Major (B)", keys: major }] as row (row.name)}
    <div class="row-label muted">{row.name}</div>
    <div class="wheel" role="group" aria-label={row.name}>
      {#each row.keys as k (k.name)}
        <button
          type="button"
          class="key"
          class:on={choice === k.name}
          aria-pressed={choice === k.name}
          title={spellings(k)}
          style:--hue={keyColour({ num: k.camelotNumber, letter: k.minor ? "A" : "B", code: k.camelot })}
          onclick={() => pick(k.name)}
        >
          <span class="code">{k.camelot}</span>
          <span class="name">{k.name}</span>
          <span class="alt muted">{k.flat ?? ""}</span>
        </button>
      {/each}
    </div>
  {/each}

  <label class="typed">
    Or type a key
    <input
      value={typed}
      oninput={(e) => onType(e.currentTarget.value)}
      placeholder="Am, F#m, Bb minor"
      maxlength="12"
      aria-invalid={typedBad}
      onkeydown={(e) => e.key === "Enter" && apply()}
    />
  </label>
  {#if typedBad}<p class="err" role="alert">Not a musical key. Use a note and major or minor, such as Am, F#m or Bb. Camelot codes like 8A are picked above.</p>{/if}

  <p class="what">
    {#if chosen}
      Sets the key to <strong>{chosen.name}</strong> ({chosen.camelot}) on {tracks.length === 1 ? "this track" : `these ${tracks.length} tracks`}. Only the key field of the library changes.
    {:else}
      Choose a key. Only the key field of the library changes; the audio and the beat grid are not touched.
    {/if}
  </p>
  <ul class="names" aria-label="Tracks that will change">
    {#each tracks as t (t.track_id)}
      <li>
        {trackLabel(t)}: {label(t.key)} →
        {#if chosen && canonicalKeyName(t.key) === chosen.name}
          <span class="muted">already {chosen.name}, unchanged</span>
        {:else}
          {chosen ? `${chosen.name} (${chosen.camelot})` : "?"}
        {/if}
      </li>
    {/each}
  </ul>

  {#snippet actions()}
    <Button data-autofocus disabled={saving} onclick={() => analysis.closeDialog()}>Cancel</Button>
    <Button variant="primary" disabled={!choice || saving} onclick={apply}>
      {saving ? "Saving" : tracks.length === 1 ? "Set key" : `Set key of ${tracks.length}`}
    </Button>
  {/snippet}
</Modal>

<style>
  .row-label {
    margin: 4px 0 4px;
    font-size: 12px;
  }
  .wheel {
    display: grid;
    grid-template-columns: repeat(6, 1fr);
    gap: 4px;
    margin-bottom: 8px;
  }
  .key {
    display: flex;
    flex-direction: column;
    align-items: center;
    padding: 3px 0 2px;
    background: var(--deep);
    color: var(--text);
    border: 1px solid var(--line);
    border-radius: 4px;
    font: inherit;
    line-height: 1.15;
    cursor: pointer;
  }
  .key:hover {
    border-color: var(--line-hi);
  }
  .key.on {
    border-color: var(--hue);
    background: color-mix(in srgb, var(--hue) 20%, var(--deep));
  }
  .code {
    color: var(--hue);
    font-size: 12px;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
  }
  .alt {
    min-height: 1.15em;
    font-size: 10px;
  }
  .typed {
    display: flex;
    flex-direction: column;
    gap: 4px;
    margin: 8px 0;
  }
  .what {
    margin: 8px 0 6px;
    color: var(--muted);
  }
  .err {
    margin: 0 0 8px;
    color: var(--bad-text);
    font-size: 12px;
  }
  .names {
    margin: 0;
    padding-left: 18px;
    max-height: 150px;
    overflow: auto;
  }
</style>
