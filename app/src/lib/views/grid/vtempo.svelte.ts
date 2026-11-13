// Variable-tempo tools of the beat grid editor: the tempo sections of the working grid, the
// comparison of its beat lines with the audio (drift), tap tempo, and the edits that change the
// tempo map. The grid itself, its undo history and the save stay in `editor` (editor.svelte.ts);
// the maths is all on the backend (pm_formats::ops_vtempo), so what is drawn before an edit and
// what is measured after it come from one function. The user's settings are remembered in the
// browser storage of the app and every number in them has a default.
import { toast } from "$lib/state/toast.svelte";
import { session } from "$lib/state/session.svelte";
import {
  gridDrift,
  gridOnsets,
  gridSections,
  type DriftOptions,
  type DriftReport,
  type OnsetList,
  type Section,
  type TempoEdit,
} from "$lib/api/grid";
import { editor } from "./editor.svelte";
import { loadPref, savePref } from "$lib/state/prefStore";
import {
  addTap,
  DEFAULT_VTEMPO,
  evenRange,
  normaliseVTempo,
  sectionRange,
  tapBpm,
  validateVTempo,
  VTEMPO_STORAGE,
  type VTempoSettings,
} from "./vtempoLogic";

function load(): VTempoSettings {
  return loadPref(
    VTEMPO_STORAGE,
    (raw) => {
      const s = normaliseVTempo(raw);
      return validateVTempo(s) ? { ...DEFAULT_VTEMPO } : s;
    },
    () => ({ ...DEFAULT_VTEMPO }),
  );
}

class VTempo {
  settings = $state<VTempoSettings>(load());
  /** Beat lines can be dragged on the waveform. */
  editBeats = $state(false);
  /** The beat line the single-beat tools act on, an index into the working grid. */
  selected = $state<number | null>(null);

  sections = $state.raw<Section[]>([]);
  sectionsError = $state("");

  onsets = $state.raw<OnsetList | null>(null);
  onsetsLoading = $state(false);
  onsetsError = $state("");
  drift = $state.raw<DriftReport | null>(null);
  driftError = $state("");

  taps = $state.raw<number[]>([]);
  tapValue = $derived(tapBpm(this.taps));

  #sectionRun = 0;
  #driftRun = 0;
  #onsetRun = 0;

  driftOptions = $derived<DriftOptions>({
    window_pct: this.settings.drift_window_pct,
    window_max_ms: this.settings.drift_window_max_ms,
    threshold_ms: this.settings.drift_threshold_ms,
    min_run: this.settings.drift_min_run,
    max_gap: this.settings.drift_max_gap,
  });

  /** Saves new settings (the caller has checked them with `validateVTempo`). */
  save(next: VTempoSettings) {
    this.settings = { ...next };
    savePref(VTEMPO_STORAGE, next);
  }

  resetSettings() {
    this.save({ ...DEFAULT_VTEMPO });
  }

  /** A different track was opened (or none): forget everything measured on the old one. */
  forget() {
    this.#sectionRun++;
    this.#driftRun++;
    this.#onsetRun++;
    this.sections = [];
    this.sectionsError = "";
    this.onsets = null;
    this.onsetsLoading = false;
    this.onsetsError = "";
    this.drift = null;
    this.driftError = "";
    this.selected = null;
    this.taps = [];
  }

  /** Cuts the working grid into sections. Stale answers (the grid changed meanwhile) are dropped. */
  async refreshSections() {
    const doc = editor.doc;
    const l = editor.loaded;
    const mine = ++this.#sectionRun;
    if (!doc || !l) {
      this.sections = [];
      this.sectionsError = "";
      return;
    }
    try {
      const s = await gridSections(
        doc,
        l.sample_rate,
        { tolerance_pct: this.settings.section_tolerance_pct, min_beats: this.settings.section_min_beats },
        editor.splits,
      );
      if (mine === this.#sectionRun) {
        this.sections = s;
        this.sectionsError = "";
      }
    } catch (e) {
      if (mine === this.#sectionRun) {
        // better no list than one that describes an older grid
        this.sections = [];
        this.sectionsError = String(e);
      }
    }
  }

  /** The drift report, only while it still describes the working grid. An insert or a remove changes
   *  the beat count before the new report arrives, and a report indexed by beat would then be drawn
   *  against the wrong lines, so it is not used until the count matches again. */
  get currentDrift(): DriftReport | null {
    const d = this.drift;
    return d && d.beats === (editor.doc?.beats.length ?? -1) ? d : null;
  }

  /** Measures the working grid against the onsets, when they have been found. */
  async refreshDrift() {
    const doc = editor.doc;
    const l = editor.loaded;
    const on = this.onsets;
    const mine = ++this.#driftRun;
    if (!doc || !l || !on) {
      this.drift = null;
      this.driftError = "";
      return;
    }
    try {
      const r = await gridDrift(doc, on.onsets, on.sample_rate, this.driftOptions);
      if (mine === this.#driftRun) {
        this.drift = r;
        this.driftError = "";
      }
    } catch (e) {
      if (mine === this.#driftRun) {
        this.drift = null;
        this.driftError = String(e);
      }
    }
  }

  /** Decodes the audio and finds its attacks (a few seconds); the drift then follows by itself. */
  async checkAudio() {
    const lib = session.library;
    const l = editor.loaded;
    if (!lib || !l || this.onsetsLoading) return;
    const mine = ++this.#onsetRun;
    this.onsetsLoading = true;
    this.onsetsError = "";
    try {
      const list = await gridOnsets(lib.root, l.track_id, this.settings.onset_min_strength);
      if (mine !== this.#onsetRun) return;
      if (list.sample_rate !== l.sample_rate) {
        this.onsetsError = `The audio is ${list.sample_rate} Hz but the library row says ${l.sample_rate} Hz, so the beat positions cannot be compared with it.`;
        return;
      }
      this.onsets = list;
    } catch (e) {
      if (mine === this.#onsetRun) this.onsetsError = String(e);
    } finally {
      if (mine === this.#onsetRun) this.onsetsLoading = false;
    }
  }

  // ---- tap tempo ----

  tap(now = performance.now()) {
    this.taps = addTap(this.taps, now, this.settings.tap_reset_secs, this.settings.tap_count);
  }

  clearTaps() {
    this.taps = [];
  }

  // ---- edits ----

  get beatCount() {
    return editor.doc?.beats.length ?? 0;
  }

  setSectionBpm(s: Section, bpm: number) {
    const r = sectionRange(s, this.beatCount);
    return editor.tempoEdit({ kind: "set_section_bpm", ...r, bpm });
  }

  async evenSection(s: Section) {
    const r = evenRange(s, this.beatCount);
    if (!r) {
      toast.show("This section has no beat between its two ends to space out", "bad");
      return false;
    }
    return editor.tempoEdit({ kind: "even", ...r });
  }

  /** Moves one beat line to `frame`; `following` moves every later beat with it. */
  moveBeat(beat: number, frame: number, following: boolean) {
    const doc = editor.doc;
    if (!doc || !doc.beats[beat]) return Promise.resolve(false);
    const e: TempoEdit = following
      ? { kind: "shift_from", beat, frames: frame - doc.beats[beat].pos }
      : { kind: "move_beat", beat, frame };
    return editor.tempoEdit(e);
  }

  nudgeSelected(ms: number) {
    const doc = editor.doc;
    const i = this.selected;
    if (!doc || i === null || !doc.beats[i]) return Promise.resolve(false);
    return this.moveBeat(i, doc.beats[i].pos + Math.round((ms * editor.sampleRate) / 1000), false);
  }

  insertBeat(frame: number) {
    return editor.tempoEdit({ kind: "insert_beat", frame });
  }

  async removeSelected() {
    const i = this.selected;
    if (i === null) return false;
    const ok = await editor.tempoEdit({ kind: "remove_beat", beat: i });
    if (ok) this.selected = null;
    return ok;
  }

  /** Cuts the grid at the beat nearest `secs`. The first beat already starts the first section. */
  splitAt(secs: number) {
    const doc = editor.doc;
    if (!doc) return;
    const frame = Math.round(secs * editor.sampleRate);
    let k = 0;
    doc.beats.forEach((b, i) => {
      if (Math.abs(b.pos - frame) < Math.abs(doc.beats[k].pos - frame)) k = i;
    });
    if (k === 0) {
      toast.show("The first beat already starts a section: put the playhead further on", "bad");
      return;
    }
    editor.addSplit(doc.beats[k].pos);
  }

  /** Moves the beats of `from..to` (to: exclusive, null = the end) onto the onsets they drift from. */
  follow(from: number, to: number | null) {
    const on = this.onsets;
    if (!on) {
      toast.show("Check the grid against the audio first", "bad");
      return Promise.resolve(false);
    }
    return editor.tempoEdit({
      kind: "follow_onsets",
      from_beat: from,
      to_beat: to,
      onsets: on.onsets,
      options: this.driftOptions,
      smooth_beats: this.settings.follow_smooth_beats,
    });
  }

  /** Moves the whole grid by the median offset: the "sits early / late as a whole" fix. */
  nudgeByBias() {
    const d = this.currentDrift;
    if (!d) return Promise.resolve(false);
    return editor.edit({ kind: "nudge", frames: Math.round((d.bias_ms * editor.sampleRate) / 1000) });
  }
}

export const vtempo = new VTempo();
session.onReset(() => vtempo.forget());
