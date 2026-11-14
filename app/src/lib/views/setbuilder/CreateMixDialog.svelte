<script lang="ts">
  import { onMount, untrack } from "svelte";
  import { jobs } from "$lib/state/jobs.svelte";
  import { mixes } from "$lib/state/mixes.svelte";
  import { nav } from "$lib/state/nav.svelte";
  import { session } from "$lib/state/session.svelte";
  import { toast } from "$lib/state/toast.svelte";
  import Button from "$lib/ui/Button.svelte";
  import Modal from "$lib/ui/Modal.svelte";
  import { recipePrefs } from "$lib/views/mixes/recipePrefs.svelte";
  import { blendLimit, createMix, planFor, preflight, transitionWords, type Created, type Preflight } from "./createMix";
  import { setPrefs } from "./prefs.svelte";
  import { setBuilder } from "./setbuilder.svelte";
  import { clock, longestRunTempo, planRuns, playSecs, solverOptions, tempoSentence, type SetTrack } from "./solver";

  // Lays the set out as a NEW mix: every track a clip, each beat-matched to the one before and
  // joined with the Mixes view's auto-transition recipe. The dialog first asks the backend about
  // every track and shows what will be done to each hop; the mix is built in memory and checked
  // before anything is written, and a failure leaves nothing behind.
  let { onclose }: { onclose: () => void } = $props();

  const root = untrack(() => session.library?.root ?? "");
  // the set as it is now; the dialog keeps working on this even if the view changes behind it
  const order: SetTrack[] = untrack(() => setBuilder.order.map((id) => setBuilder.byId.get(id)).filter((t): t is SetTrack => !!t));
  const titleOf = (id: number) => setBuilder.rows.get(id)?.title || `track #${id}`;

  let phase = $state<"loading" | "ready" | "building" | "done">("loading");
  let pre = $state.raw<Preflight | null>(null);
  let problem = $state("");
  let progress = $state("");
  let created = $state.raw<Created | null>(null);

  let prefs = $derived(setPrefs.value);
  let plan = $derived(pre ? planFor(pre, prefs) : null);
  let o = $derived(solverOptions(prefs));
  let taken = $derived(new Set((session.library?.mixes ?? []).map((m) => m.name.trim().toLowerCase())));

  function freeName(base: string): string {
    if (!taken.has(base.toLowerCase())) return base;
    for (let n = 2; ; n++) if (!taken.has(`${base} ${n}`.toLowerCase())) return `${base} ${n}`;
  }
  let name = $state(untrack(() => freeName(`${setBuilder.source?.kind === "case" ? setBuilder.source.name : "Set"} mix`)));
  let nameOk = $derived(name.trim().length > 0 && [...name].length <= 255 && !taken.has(name.trim().toLowerCase()));

  onMount(() => {
    void (async () => {
      try {
        pre = await preflight(root, order, setPrefs.value);
        phase = "ready";
      } catch (e) {
        problem = `Could not read the tracks: ${e instanceof Error ? e.message : String(e)}`;
        phase = "ready";
      }
    })();
  });

  let sequences = $derived((plan?.pairs ?? []).filter((p) => p.mode === "sequence"));
  let biggest = $derived.by(() => {
    if (!plan || !pre) return null;
    let best: { id: number; pct: number } | null = null;
    pre.usable.forEach((t, i) => {
      const pct = plan!.stretchPct[i] ?? 0;
      if (Math.abs(pct) > Math.abs(best?.pct ?? 0)) best = { id: t.id, pct };
    });
    return best as { id: number; pct: number } | null;
  });
  let sumSecs = $derived(pre ? pre.usable.reduce((s, t) => s + playSecs(t, o), 0) : 0);

  const sign = (n: number) => `${n >= 0 ? "+" : "−"}${Math.abs(n).toFixed(1)} %`;

  // The row already names the track the hop goes to ("“T”: ..."), so a reason about that same track
  // says "this track" instead of repeating the title.
  function why(p: { to: number; why: string | null; culprit: number | null; jumpPct?: number }): string {
    const who = p.culprit === p.to ? "This track" : `“${titleOf(p.culprit ?? 0)}”`;
    if (p.why === "noGrid") return `${who} has no beat grid`;
    if (p.why === "noTempo") return `${who} has no tempo`;
    if (p.why === "bigJump") return `${who} is ${Math.abs(p.jumpPct ?? 0).toFixed(1)} % ${(p.jumpPct ?? 0) >= 0 ? "faster" : "slower"} than the track before it, over the ${blendLimit(prefs)} % limit for beat-matching`;
    return `${who} is at half or double the tempo of the track before it`;
  }

  /** A mix opens with its first clip's tempo as the reference tempo ("Match all" pulls the others
   *  toward it), which is wrong when that clip is a run of one at its own, perhaps mis-analysed,
   *  tempo. The reference is not saved, so it is set once the new mix is open (the Mixes view opens
   *  it through the deep link): the tempo of the longest beat-matched run. Gives up after 20 s. */
  function setTempoWhenOpen(id: number, bpm: number) {
    let stop: (() => void) | null = null;
    let done = false;
    const finish = () => {
      done = true;
      clearTimeout(timer);
      queueMicrotask(() => stop?.());
    };
    const timer = setTimeout(finish, 20000);
    stop = $effect.root(() => {
      $effect(() => {
        if (done || mixes.doc?.id !== id) return;
        untrack(() => mixes.setTempo(Math.round(bpm * 100) / 100));
        finish();
      });
    });
  }

  function goToMixes() {
    if (created) {
      const t = prefs.beatmatch && plan ? longestRunTempo(plan) : null;
      if (t !== null) setTempoWhenOpen(created.saved.id, t);
      nav.go("mixes", { "mixes.mixId": created.saved.id });
    } else nav.go("mixes");
    onclose();
  }

  async function go() {
    if (!pre || !plan || !nameOk || phase !== "ready" || !jobs.guard()) return;
    phase = "building";
    problem = "";
    progress = "Starting…";
    try {
      created = await createMix(root, name.trim(), pre, plan, titleOf, prefs, recipePrefs.value, (done, total, title) => {
        progress = done >= total ? "Checking and saving…" : `Placing track ${done + 1} of ${total}: ${title}`;
      });
      try {
        await mixes.refreshList();
      } catch {
        // the list refreshes next time the Mixes view opens
      }
      toast.show(`Created the mix “${created.saved.name}”`, "ok", 6000);
      phase = "done";
    } catch (e) {
      problem = `The mix was not created: ${e instanceof Error ? e.message : String(e)}`;
      phase = "ready";
    }
  }
</script>

<Modal title="Create a mix from the set" onclose={() => phase !== "building" && onclose()}>
  {#if phase === "done" && created}
    {@const r = created.result}
    <p>
      Created <b>“{created.saved.name}”</b>: {r.clipIds.length} track{r.clipIds.length === 1 ? "" : "s"}, {clock(created.saved.playTimeSecs)}.
      {#if r.toolsSkipped > 0}{r.toolsSkipped} transition tool{r.toolsSkipped === 1 ? " was" : "s were"} left out because another tool was in the way.{/if}
    </p>
    {#if r.notes.length > 0}
      <p class="lbl">Worth a look</p>
      <ul>{#each r.notes as n, i (i)}<li>{n}</li>{/each}</ul>
    {/if}
    {@const worst = r.stretchPct.reduce((m, x, i) => (Math.abs(x) > Math.abs(m.pct) ? { pct: x, id: i } : m), { pct: 0, id: -1 })}
    {#if worst.id >= 0 && Math.abs(worst.pct) >= 0.05}
      <p class="muted">Largest tempo change: {sign(worst.pct)} on “{titleOf(pre?.usable[worst.id]?.id ?? 0)}”.</p>
    {/if}
    <p class="muted">Open it in the Mixes view to listen and adjust. The mix is saved in the library like any other.</p>
  {:else}
    <form
      onsubmit={(e) => {
        e.preventDefault();
        void go();
      }}
    >
      <!-- svelte-ignore a11y_autofocus -->
      <input data-autofocus bind:value={name} maxlength="255" aria-label="Mix name" placeholder="Mix name" disabled={phase === "building"} />
    </form>
    {#if name.trim() && taken.has(name.trim().toLowerCase())}
      <p class="warn" role="status">A mix with this name already exists. Pick another name.</p>
    {/if}

    {#if phase === "loading"}
      <p class="muted">Checking the tracks…</p>
    {:else if pre && plan}
      <p>
        {pre.usable.length} track{pre.usable.length === 1 ? "" : "s"}, about {clock(sumSecs)} before the transitions overlap them.
        Transitions: {transitionWords(recipePrefs.value)} (the Mixes view's settings). A blend overlaps by at most {prefs.overlapCapPct} % of the shorter track; a tool that overlaps one an earlier transition placed is {prefs.onToolConflict === "replace" ? "replaced" : "left out"} (set in the builder's settings).
        {#if o.maxTrackSecs != null}Each track is cut to at most {clock(o.maxTrackSecs)}, on a whole bar.{/if}
      </p>

      {#if pre.unusable.length > 0}
        <p class="lbl bad">Cannot go in a mix, so left out ({pre.unusable.length})</p>
        <ul>{#each pre.unusable as u (u.id)}<li>“{titleOf(u.id)}”: {u.why}</li>{/each}</ul>
      {/if}

      <label class="check">
        <input type="checkbox" checked={prefs.beatmatch} disabled={phase === "building"} onchange={(e) => setPrefs.set({ beatmatch: e.currentTarget.checked })} />
        <span>Beat-match each track to the one before it</span>
      </label>
      <label class="row">
        <span>Tempo the mix runs at (BPM)</span>
        <input type="number" min="40" max="250" step="0.1" placeholder={planRuns(plan).filter((r) => r.count > 1 && r.tempo !== null).length > 1 ? "middle tempo of each stretch" : "middle tempo of the set"} disabled={!prefs.beatmatch || phase === "building"} value={prefs.mixTempo ?? ""} onchange={(e) => setPrefs.commitNumber(e.currentTarget, "mixTempo", true)} />
      </label>
      <label class="row">
        <span>Do not beat-match a hop bigger than (%)</span>
        <input type="number" min="0" max="50" step="0.5" placeholder="the set's largest tempo jump ({prefs.maxBpmJumpPct} %)" disabled={!prefs.beatmatch || phase === "building"} value={prefs.blendJumpPct ?? ""} onchange={(e) => setPrefs.commitNumber(e.currentTarget, "blendJumpPct", true)} />
      </label>

      {#if prefs.beatmatch && biggest && Math.abs(biggest.pct) >= 0.05}
        <p class="muted">{tempoSentence(plan, prefs.mixTempo == null)} Largest change: {sign(biggest.pct)} on “{titleOf(biggest.id)}”.</p>
      {:else if !prefs.beatmatch}
        <p class="muted">Tracks keep their own tempo and are only overlapped and crossfaded, so the beats will not line up.</p>
      {/if}

      {#if sequences.length > 0}
        <p class="lbl">Start when the track before ends, no overlap ({sequences.length})</p>
        <ul>{#each sequences as p (p.to)}<li>“{titleOf(p.to)}”: {why(p)}</li>{/each}</ul>
      {/if}
    {/if}

    {#if phase === "building"}<p role="status">{progress}</p>{/if}
    {#if problem}<p class="warn" role="alert">{problem}</p>{/if}

  {/if}
  {#snippet actions()}
    {#if phase === "done"}
      <Button onclick={onclose}>Close</Button>
      <Button variant="primary" onclick={goToMixes}>Go to Mixes</Button>
    {:else}
      <Button onclick={onclose} disabled={phase === "building"}>Cancel</Button>
      <Button variant="primary" disabled={phase !== "ready" || !pre || pre.usable.length === 0 || !nameOk || !jobs.canWrite} onclick={go}>
        {phase === "building" ? "Creating…" : "Create mix"}
      </Button>
    {/if}
  {/snippet}
</Modal>

<style>
  input:not([type]) {
    width: 100%;
    box-sizing: border-box;
  }
  p {
    margin: 8px 0 0;
  }
  ul {
    margin: 4px 0 0;
    padding: 6px 6px 6px 22px;
    max-height: 160px;
    overflow: auto;
    background: var(--deep);
    border: 1px solid var(--line);
    border-radius: 4px;
    font-size: 12px;
  }
  .lbl {
    font-size: 11px;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--muted);
    margin-top: 12px;
  }
  .bad,
  .warn {
    color: var(--bad-text);
  }
  .check {
    display: flex;
    gap: 6px;
    margin-top: 12px;
    align-items: center;
  }
  .row {
    display: grid;
    gap: 2px;
    margin-top: 8px;
    font-size: 12px;
  }
  .row input {
    width: 100%;
    box-sizing: border-box;
  }
</style>
