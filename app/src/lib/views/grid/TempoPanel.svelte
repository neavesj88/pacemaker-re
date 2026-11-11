<script lang="ts">
  import { untrack } from "svelte";
  import { toast } from "$lib/state/toast.svelte";
  import Button from "$lib/ui/Button.svelte";
  import type { Section } from "$lib/api/grid";
  import { editor } from "./editor.svelte";
  import { fmtTime } from "./gridMath";
  import { vtempo } from "./vtempo.svelte";
  import { biasNote, DEFAULT_VTEMPO, evenRange, sectionOfBeat, spanLine, validateVTempo, type VTempoSettings } from "./vtempoLogic";

  // The variable-tempo tools: the sections of the grid with a tempo each, tap tempo, single beat
  // lines, and the grid checked against the audio. Everything it changes goes through `editor`
  // (undoable, nothing is saved until the editor's Save).
  let { head, onseek }: { head: number; onseek: (secs: number) => void } = $props();

  const sr = $derived(editor.sampleRate);
  const n = $derived(editor.doc?.beats.length ?? 0);
  const canEdit = $derived(!!editor.doc && !editor.busy);
  /** The sections above the user's limit are not listed (a grid with a wobbling tempo has hundreds). */
  const listMax = $derived(vtempo.settings.section_list_max);

  // ---- sections ----
  // an unsaved typed tempo belongs to one section: keyed by where it starts, so after the sections
  // are recomputed it cannot show up against a different one
  let drafts = $state<Record<string, string>>({});
  const draftKey = (s: Section) => `${s.first_beat}:${s.start_frame}`;
  $effect(() => {
    const live = new Set(vtempo.sections.map(draftKey));
    untrack(() => {
      for (const k of Object.keys(drafts)) if (!live.has(k)) delete drafts[k];
    });
  });
  const headBeat = $derived.by(() => {
    const d = editor.doc;
    if (!d) return 0;
    const f = head * sr;
    let k = 0;
    d.beats.forEach((b, i) => {
      if (b.pos <= f + 1) k = i;
    });
    return k;
  });
  const current = $derived(sectionOfBeat(vtempo.sections, headBeat));
  const shown = $derived(vtempo.sections.slice(0, listMax));

  function shownBpm(s: Section) {
    return drafts[draftKey(s)] ?? s.bpm.toFixed(2);
  }

  async function applySection(s: Section) {
    const v = Number((drafts[draftKey(s)] ?? s.bpm.toFixed(2)).replace(",", "."));
    if (!Number.isFinite(v) || v <= 0) {
      toast.show("Enter a tempo in BPM, for example 128 or 127.5", "bad");
      return;
    }
    if (Math.abs(v - s.bpm) < 0.005) {
      delete drafts[draftKey(s)];
      return;
    }
    if (await vtempo.setSectionBpm(s, v)) delete drafts[draftKey(s)];
  }

  const splitAtFrame = (s: Section) => s.start_frame;

  // ---- tap tempo ----
  const tapNow = $derived(vtempo.tapValue);
  const tapSection = $derived(current >= 0 ? vtempo.sections[current] : null);

  async function useTap(round: boolean) {
    const s = tapSection;
    const t = tapNow;
    if (!s || t === null) return;
    const bpm = round ? Math.round(t) : Math.round(t * 100) / 100;
    if (await vtempo.setSectionBpm(s, bpm)) vtempo.clearTaps();
  }

  // ---- beat lines ----
  const sel = $derived(vtempo.selected !== null && editor.doc?.beats[vtempo.selected] ? vtempo.selected : null);
  const headFrame = $derived(Math.round(head * sr));

  // ---- drift ----
  const d = $derived(vtempo.currentDrift);
  const bias = $derived(d ? biasNote(d.bias_ms) : null);

  // ---- settings ----
  let draft = $state<VTempoSettings>({ ...vtempo.settings });
  let settingsError = $state("");
  const fields: { key: keyof VTempoSettings; label: string; unit: string; step: number; hint: string }[] = [
    { key: "section_tolerance_pct", label: "New section when the beat gap changes by", unit: "%", step: 0.1, hint: "Beats closer in tempo than this are one section." },
    { key: "section_min_beats", label: "Shortest section", unit: "beats", step: 1, hint: "A shorter run is folded into its neighbour, unless you split there." },
    { key: "drift_threshold_ms", label: "A beat is off by more than", unit: "ms", step: 1, hint: "Measured from the median offset of the whole grid." },
    { key: "drift_min_run", label: "Drift needs", unit: "beats in a row", step: 1, hint: "Fewer off beats in a row are not reported." },
    { key: "drift_max_gap", label: "A drift span may bridge", unit: "beats with no onset", step: 1, hint: "Beats with nothing near them inside a run do not end it." },
    { key: "onset_min_strength", label: "Ignore onsets weaker than", unit: "(flux deviations)", step: 0.5, hint: "Applies the next time the audio is checked. Higher ignores quieter hits." },
    { key: "drift_window_pct", label: "Match an onset within", unit: "% of a beat", step: 1, hint: "How far from a beat line an onset may be to belong to it." },
    { key: "drift_window_max_ms", label: "...and within at most", unit: "ms", step: 5, hint: "Whichever of the two windows is smaller applies." },
    { key: "follow_smooth_beats", label: "Follow the audio smoothed over", unit: "beats", step: 2, hint: "Each beat moves by the median correction of the beats around it." },
    { key: "grab_px", label: "Grab a beat line within", unit: "px", step: 1, hint: "How close the pointer must be to pick up a beat line." },
    { key: "section_list_max", label: "List at most", unit: "sections", step: 50, hint: "A longer list is cut off with a note." },
    { key: "tap_count", label: "Tap tempo averages the last", unit: "taps", step: 1, hint: "More taps give a steadier number." },
    { key: "tap_reset_secs", label: "Tap count restarts after a pause of", unit: "s", step: 0.5, hint: "" },
  ];

  function applySettings() {
    const bad = validateVTempo(draft);
    settingsError = bad;
    if (bad) return;
    vtempo.save(draft);
    toast.show("Tempo tool settings saved", "ok");
  }

  function resetSettings() {
    draft = { ...DEFAULT_VTEMPO };
    settingsError = "";
    vtempo.resetSettings();
  }

  const settingsChanged = $derived(JSON.stringify(draft) !== JSON.stringify(vtempo.settings));

  function setNum(key: keyof VTempoSettings, v: string) {
    const x = Number(v.replace(",", "."));
    (draft as Record<string, unknown>)[key] = Number.isFinite(x) ? x : NaN;
  }
</script>

<div class="panel">
  <div class="card wide">
    <h4>
      Tempo map
      <span class="chip" class:g={vtempo.sections.length <= 1}>{vtempo.sections.length <= 1 ? "one tempo" : `${vtempo.sections.length} sections`}</span>
    </h4>
    <p class="note first">
      The .str file stores a position and a tempo for every beat, so a tempo that changes is saved exactly as you edit it.
      That the Pacemaker plays each beat where the file puts it (rather than smoothing the tempo) has not been tested on
      a device.
    </p>
    {#if vtempo.sectionsError}<p class="err">{vtempo.sectionsError}</p>{/if}
    {#if vtempo.sections.length > 0}
      <div class="scroll">
        <table>
          <thead>
            <tr><th>#</th><th>From</th><th class="r">Beats</th><th>Tempo</th><th></th></tr>
          </thead>
          <tbody>
            {#each shown as s, i (s.first_beat)}
              <tr class:here={i === current}>
                <td class="num">{i + 1}</td>
                <td>
                  <button class="go num" title="Move the playhead to the start of this section" onclick={() => onseek(s.start_frame / sr)}>{fmtTime(s.start_frame / sr)}</button>
                  {#if s.forced}<span class="tag" title="You cut the grid here">split</span>{/if}
                </td>
                <td class="num r">{s.first_beat + 1} to {s.first_beat + s.beats}</td>
                <td class="tempo">
                  <input
                    class="bpm num"
                    aria-label="Tempo of section {i + 1} in BPM"
                    inputmode="decimal"
                    disabled={!canEdit}
                    value={shownBpm(s)}
                    oninput={(e) => (drafts[draftKey(s)] = e.currentTarget.value)}
                    onkeydown={(e) => e.key === "Enter" && applySection(s)}
                  />
                  {#if s.max_bpm - s.min_bpm > 0.05}
                    <span class="muted num" title="Slowest and fastest beat gap in this section">{s.min_bpm.toFixed(2)} to {s.max_bpm.toFixed(2)}</span>
                  {/if}
                </td>
                <td class="acts">
                  <Button
                    small
                    disabled={!canEdit || drafts[draftKey(s)] === undefined || Math.abs(Number(drafts[draftKey(s)].replace(',', '.')) - s.bpm) < 0.005}
                    onclick={() => applySection(s)}
                    title="Rebuild this section's beats at this tempo, starting on its first beat. The next section does not move."
                  >Set</Button>
                  <Button
                    small
                    disabled={!canEdit || !evenRange(s, n)}
                    onclick={() => vtempo.evenSection(s)}
                    title="Space this section's beats evenly between its first beat and the next section's first beat"
                  >Even</Button>
                  {#if s.forced}
                    <Button small onclick={() => editor.removeSplit(splitAtFrame(s))} title="Remove the cut (the tempo is not changed)">Unsplit</Button>
                  {/if}
                </td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
      {#if vtempo.sections.length > listMax}
        <p class="note">
          Showing the first {listMax} of {vtempo.sections.length} sections. A grid with this many tempo changes follows
          a wandering beat: raise the section tolerance in the settings below to group them.
        </p>
      {/if}
    {:else}
      <p class="muted">No beat grid to show.</p>
    {/if}
    <div class="row">
      <Button small disabled={!canEdit} onclick={() => vtempo.splitAt(head)} title="Cut the grid into a new section at the beat nearest the playhead, then give that section its own tempo">Split at playhead</Button>
      <span class="muted">Splitting changes nothing by itself; set the tempo of the new section in the table.</span>
    </div>
  </div>

  <div class="card">
    <h4>Tap tempo</h4>
    <div class="row">
      <Button
        onpointerdown={(e: PointerEvent) => {
          // tap on the press, not the release: that is when the beat was felt
          if (e.button === 0) {
            e.preventDefault();
            vtempo.tap();
          }
        }}
        onclick={(e: MouseEvent) => {
          // a click from the keyboard (Enter, Space) has no pointer; a mouse click was counted on the press
          if (e.detail === 0) vtempo.tap();
        }}
        title="Tap along with the music (or press T)">Tap</Button
      >
      <span class="big num">{tapNow === null ? "-" : tapNow.toFixed(2)}</span>
      <span class="muted">BPM · {vtempo.taps.length} tap{vtempo.taps.length === 1 ? "" : "s"}</span>
      <Button small disabled={vtempo.taps.length === 0} onclick={() => vtempo.clearTaps()}>Clear</Button>
    </div>
    <div class="row">
      <Button small disabled={!canEdit || tapNow === null || !tapSection} onclick={() => useTap(false)}>
        Set section {current + 1} to {tapNow === null ? "tap" : tapNow.toFixed(2)}
      </Button>
      <Button small disabled={!canEdit || tapNow === null || !tapSection} onclick={() => useTap(true)}>
        Whole BPM{tapNow === null ? "" : ` (${Math.round(tapNow)})`}
      </Button>
    </div>
    <p class="note">
      The section under the playhead takes the tempo. Press <kbd>T</kbd> to tap from the keyboard. The tap count restarts
      after a pause (settings below).
    </p>
  </div>

  <div class="card">
    <h4>Beat lines</h4>
    <div class="row">
      <Button small active={vtempo.editBeats} disabled={!editor.doc} onclick={() => (vtempo.editBeats = !vtempo.editBeats)} title="Grab beat lines on the waveform to move them">
        {vtempo.editBeats ? "Dragging is on" : "Drag beat lines"}
      </Button>
      <Button small disabled={!canEdit} onclick={() => vtempo.insertBeat(headFrame)} title="Add a beat line at the playhead">Add at playhead</Button>
    </div>
    <div class="row">
      {#if sel !== null && editor.doc}
        <span>Beat <span class="num">{sel + 1}</span> at <span class="num">{fmtTime(editor.doc.beats[sel].pos / sr)}</span></span>
      {:else}
        <span class="muted">No beat line selected (click one while dragging is on).</span>
      {/if}
    </div>
    <div class="row">
      {#each [-5, -1, 1, 5] as ms (ms)}
        <Button small disabled={!canEdit || sel === null} onclick={() => vtempo.nudgeSelected(ms)} title="Move the selected beat line {ms < 0 ? 'earlier' : 'later'} by {Math.abs(ms)} ms">
          {ms > 0 ? "+" : "−"}{Math.abs(ms)} ms
        </Button>
      {/each}
      <Button small disabled={!canEdit || sel === null || n < 2} onclick={() => vtempo.removeSelected()} title="Delete the selected beat line">Remove</Button>
    </div>
    <p class="note">
      Drag a line to move just that one. Alt+drag moves it and every later beat together. A line stays clear of its
      neighbours by the shortest beat the device allows (400 BPM).
    </p>
  </div>

  <div class="card wide">
    <h4>
      Drift against the audio
      {#if d}
        <span class="chip" class:g={d.spans.length === 0} class:bad={d.spans.length > 0}>
          {d.spans.length === 0 ? "no drift" : `${d.spans.length} span${d.spans.length === 1 ? "" : "s"} off`}
        </span>
      {/if}
    </h4>
    <div class="row">
      <Button small disabled={!editor.doc || vtempo.onsetsLoading} onclick={() => vtempo.checkAudio()} title="Reads the audio file and compares its attacks with the beat lines (a few seconds)">
        {vtempo.onsetsLoading ? "Reading the audio…" : vtempo.onsets ? "Read the audio again" : "Check against the audio"}
      </Button>
      <Button
        small
        disabled={!canEdit || !vtempo.onsets}
        onclick={() => vtempo.follow(0, null)}
        title="Move every beat line onto the nearest onset, smoothed, keeping the grid's overall offset"
      >Follow the audio, whole track</Button>
    </div>
    {#if vtempo.onsetsError}<p class="err">{vtempo.onsetsError}</p>{/if}
    {#if vtempo.driftError}<p class="err">{vtempo.driftError}</p>{/if}
    {#if d}
      <p class="muted">
        {d.matched} of {d.beats} beat lines have an onset near them. Worst beat: {d.worst_ms.toFixed(0)} ms from the median
        offset ({d.bias_ms.toFixed(1)} ms).
      </p>
      {#if bias}
        <p class="row">
          <span>{bias}</span>
          <Button small disabled={!canEdit} onclick={() => vtempo.nudgeByBias()} title="Move every beat by the median offset">Nudge by {d.bias_ms.toFixed(1)} ms</Button>
        </p>
      {/if}
      {#if d.spans.length > 0}
        <ul class="spans">
          {#each d.spans as sp, i (i)}
            <li>
              <span>{spanLine(sp, sr)}</span>
              <Button small onclick={() => onseek(sp.start_frame / sr)}>Show</Button>
              <Button small disabled={!canEdit} onclick={() => vtempo.follow(sp.first_beat, sp.last_beat + 1)} title="Move just these beat lines onto the audio">Follow the audio here</Button>
            </li>
          {/each}
        </ul>
      {:else}
        <p class="muted">No run of {vtempo.settings.drift_min_run} or more beats is off by more than {vtempo.settings.drift_threshold_ms} ms.</p>
      {/if}
    {:else if !vtempo.onsetsLoading}
      <p class="muted">
        Reads the audio file and shows where its attacks fall against each beat line, so a grid that slowly slips off the
        music stands out. Nothing is changed until you choose to follow the audio, and that is undoable.
      </p>
    {/if}
  </div>

  <details class="card wide settings">
    <summary>Settings for the tempo tools</summary>
    <div class="fields">
      {#each fields as f (f.key)}
        <label title={f.hint}>
          <span>{f.label}</span>
          <input
            class="num"
            inputmode="decimal"
            value={String(draft[f.key])}
            oninput={(e) => setNum(f.key, e.currentTarget.value)}
            aria-label={f.label}
          />
          <span class="muted">{f.unit}</span>
        </label>
      {/each}
      <label class="check">
        <input type="checkbox" bind:checked={draft.auto_check} />
        <span>Check the grid against the audio when a track opens (reads the file, a few seconds)</span>
      </label>
    </div>
    {#if settingsError}<p class="err">{settingsError}</p>{/if}
    <div class="row">
      <Button small variant="primary" disabled={!settingsChanged} onclick={applySettings}>Save settings</Button>
      <Button small onclick={resetSettings}>Reset to defaults</Button>
    </div>
  </details>
</div>

<style>
  .panel {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(300px, 1fr));
    gap: 14px;
    padding: 12px 0 0;
  }
  .card {
    background: var(--panel);
    border: 1px solid var(--line);
    border-radius: 6px;
    padding: 12px;
    min-width: 0;
  }
  .wide {
    grid-column: 1 / -1;
  }
  h4 {
    margin: 0 0 8px;
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .chip {
    font-size: 11px;
    font-weight: normal;
    padding: 1px 7px;
    border-radius: 9px;
    background: var(--accent-soft);
    color: var(--accent);
  }
  .chip.g {
    background: rgba(0, 255, 70, 0.1);
    color: var(--green);
  }
  .chip.bad {
    background: var(--bad-bg);
    color: var(--bad-text);
  }
  .row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
    margin: 8px 0;
  }
  .note {
    font-size: 12px;
    color: var(--muted);
    border-top: 1px solid var(--line);
    margin: 12px 0 0;
    padding-top: 10px;
  }
  .note.first {
    border-top: none;
    margin: 0 0 10px;
    padding-top: 0;
  }
  .err {
    color: var(--bad);
    margin: 6px 0;
  }
  .scroll {
    max-height: 280px;
    overflow: auto;
    border: 1px solid var(--line);
    border-radius: 4px;
  }
  table {
    width: 100%;
    border-collapse: collapse;
  }
  th {
    position: sticky;
    top: 0;
    background: var(--panel);
    color: var(--muted);
    font-weight: normal;
    font-size: 12px;
    text-align: left;
    padding: 4px 8px;
    border-bottom: 1px solid var(--line);
  }
  td {
    padding: 3px 8px;
    border-bottom: 1px solid var(--line);
    vertical-align: middle;
  }
  tr.here td {
    background: var(--hover);
  }
  .r {
    text-align: right;
  }
  .tempo {
    white-space: nowrap;
  }
  .acts {
    display: flex;
    gap: 4px;
    justify-content: flex-end;
  }
  .bpm {
    width: 82px;
    text-align: right;
  }
  .go {
    background: none;
    border: none;
    color: var(--accent);
    cursor: pointer;
    padding: 0;
    font: inherit;
  }
  .tag {
    font-size: 11px;
    color: var(--cue);
    margin-left: 6px;
  }
  .big {
    font-size: 22px;
    min-width: 84px;
    text-align: right;
  }
  .spans {
    list-style: none;
    margin: 8px 0 0;
    padding: 0;
  }
  .spans li {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
    padding: 5px 0;
    border-top: 1px solid var(--line);
  }
  .spans li > span {
    flex: 1 1 320px;
  }
  kbd {
    border: 1px solid var(--line-hi);
    border-radius: 3px;
    padding: 0 4px;
  }
  .settings summary {
    cursor: pointer;
    color: var(--muted);
  }
  .fields {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(360px, 1fr));
    gap: 6px 18px;
    margin: 10px 0;
  }
  .fields label {
    display: grid;
    grid-template-columns: 1fr 76px auto;
    gap: 8px;
    align-items: center;
  }
  .fields input.num {
    text-align: right;
  }
  .fields .check {
    grid-template-columns: auto 1fr;
    grid-column: 1 / -1;
  }
</style>
