<script lang="ts">
  // How the timeline scrolls with the playhead while a mix plays (the "follow playhead" tick in the
  // mix editor turns it on and off; this is how it behaves). The beat grid editor's Follow uses the
  // same choice. Remembered between sessions.
  import { followPrefs } from "./followPrefs.svelte";
  import { mv } from "./view.svelte";
  import { viewPrefs } from "./viewPrefs.svelte";
  import type { FollowMode } from "./followLogic";

  const p = $derived(followPrefs.value);
  const ANCHORS = [20, 30, 40, 50, 60, 70, 80];
  const MODES: { id: FollowMode; label: string }[] = [
    { id: "smooth", label: "Follow smoothly" },
    { id: "page", label: "Jump a page at the edge" },
  ];
</script>

<div class="field">
  <span class="lbl">Following</span>
  <label class="check"><input type="checkbox" checked={viewPrefs.value.follow} onchange={(e) => { viewPrefs.set({ follow: e.currentTarget.checked }); if (e.currentTarget.checked) mv.resumeFollow(); }} /> scroll the timeline with the playhead while a mix plays</label>
</div>
<div class="field" role="radiogroup" aria-label="While a mix plays, the view">
  <span class="lbl">While playing, the view</span>
  {#each MODES as m (m.id)}
    <label class="check"><input type="radio" name="follow-mode" value={m.id} checked={p.mode === m.id} onchange={() => followPrefs.set({ mode: m.id })} /> {m.label}</label>
  {/each}
</div>
<div class="field">
  <label for="fs-anchor">Playhead sits</label>
  <select id="fs-anchor" value={p.anchorPct} disabled={p.mode !== "smooth"} onchange={(e) => followPrefs.set({ anchorPct: Number(e.currentTarget.value) })}>
    {#each ANCHORS as a (a)}<option value={a}>{a} % from the left{a === 40 ? " (default)" : ""}</option>{/each}
  </select>
</div>
<div class="muted small">
  Smoothly: the view stays put until the playhead has reached that spot, then scrolls a little at a time so the playhead stays there, and stops at the end of the mix. Jump a page: the view stays put until the playhead reaches the right edge, then jumps. Scrolling or zooming the timeline yourself pauses following until you press Play, click the ruler or use Follow again. The "follow playhead" tick in the mix editor (the same as the first choice here, and in the View menu) turns following off altogether; Follow in the beat grid editor has its own tick.
</div>

<style>
  .small {
    font-size: 12px;
    margin: 4px 0;
  }
  .muted {
    color: var(--muted);
  }
</style>
