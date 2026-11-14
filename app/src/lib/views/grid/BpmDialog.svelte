<script lang="ts">
  // Step one of "Set BPM" for one track: type the tempo. "Continue" asks the backend what the
  // edit would do and opens the usual write confirmation. The BPM is not a free field: the beat
  // grid (.str) is rewritten with it, the same as the grid editor's Set BPM.
  import { session } from "$lib/state/session.svelte";
  import { tempo } from "$lib/state/tempo.svelte";
  import { fmtBpm, MAX_BPM, MIN_BPM, parseTempo } from "$lib/state/tempoLogic";
  import Button from "$lib/ui/Button.svelte";
  import Modal from "$lib/ui/Modal.svelte";

  let { trackId }: { trackId: number } = $props();

  const track = $derived(session.library?.tracks.find((t) => t.track_id === trackId));
  let text = $state("");
  let touched = $state(false);

  // Start from the tempo the track has now, once. `track` is a new object whenever the library
  // patches that track (background key analysis, a star click), and that must not put the old
  // tempo back over what is being typed.
  let seeded = false;
  $effect.pre(() => {
    if (seeded || !track) return;
    seeded = true;
    text = track.bpm > 0 ? fmtBpm(track.bpm) : "";
  });

  const parsed = $derived(parseTempo(text));
  const error = $derived("error" in parsed ? parsed.error : "");
  const same = $derived(!error && track !== undefined && "bpm" in parsed && Math.abs(parsed.bpm - track.bpm) < 0.005);

  function go() {
    touched = true;
    if ("bpm" in parsed && !error) tempo.continueSet(trackId, parsed.bpm);
  }
</script>

<Modal title="Set the BPM" onclose={() => tempo.closeDialog()}>
  {#if track}
    <p class="who">{track.title}{track.artist ? ` · ${track.artist}` : ""}</p>
    <label class="f">
      BPM ({MIN_BPM} to {MAX_BPM})
      <span class="line">
        <input
          data-autofocus
          inputmode="decimal"
          bind:value={text}
          oninput={() => (touched = true)}
          onkeydown={(e) => e.key === "Enter" && go()}
          aria-invalid={touched && !!error}
        />
      </span>
    </label>
    {#if touched && error}<p class="err" role="alert">{error}</p>{/if}
    {#if same}<p class="muted">That is the tempo it has now. If its tempo varies, this still makes the grid even; if the grid is already like that, nothing is changed.</p>{/if}
    <p class="muted">
      The beat grid is replaced by an even grid at this tempo, starting at the track's first beat, and the BPM is stored with
      it. Tempo changes inside the track are lost. To halve or double a track and keep its own beat lines, use Halve BPM or
      Double BPM instead.
    </p>
  {/if}
  {#snippet actions()}
    <Button onclick={() => tempo.closeDialog()}>Cancel</Button>
    <Button variant="primary" disabled={!!error} onclick={go}>Continue…</Button>
  {/snippet}
</Modal>

<style>
  .who {
    margin: 0 0 10px;
    font-weight: 600;
  }
  .f {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .line {
    display: flex;
    gap: 6px;
    align-items: center;
  }
  .line input {
    flex: 1;
    min-width: 0;
  }
  p {
    margin: 8px 0 0;
  }
  .err {
    color: var(--bad-text);
    font-size: 12px;
  }
</style>
