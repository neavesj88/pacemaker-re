// Background analysis: the settings, the status line and the "couldn't analyse" list of the worker
// that makes missing waveforms, keys, energy and sections whichever view is shown (Rust side:
// commands/bganalysis.rs). Finished work arrives as events and is shown without a reload: keys are
// patched into the table rows, energy and sections are read again, and a waveform that was known
// to be missing is asked for again.
import {
  bgGet,
  bgRetryFailures,
  bgRunNow,
  bgSaveSettings,
  bgSetHold,
  bgSetPaused,
  bgSetPlaying,
  bgWatch,
  type BgChanged,
  type BgFailure,
  type BgSettings,
  type BgStatus,
} from "$lib/api/bganalysis";
import { forgetPeaks } from "$lib/views/tracks/peakCache";
import { peakTick } from "$lib/views/tracks/peakTick.svelte";
import { failureCount, failuresStale, groupKeys, libraryChanged, parseDismissed } from "$lib/views/analysis/bgLogic";
import { energy } from "./energy.svelte";
import { sections } from "./sections.svelte";
import { loadPref, savePref } from "./prefStore";
import { session } from "./session.svelte";
import { toast } from "./toast.svelte";

export const DEFAULT_BG_SETTINGS: BgSettings = {
  enabled: true,
  waveforms: true,
  keys: true,
  energy: true,
  sections: true,
  pause_during_playback: true,
  hold_during_sync_plan: true,
  key_min_confidence_pct: 0,
};

const NO_STATUS: BgStatus = { state: "no-library", detail: "", root: null, pacemaker: false, word: "", done: 0, total: 0, left: 0, label: "", failures: 0 };

/** The failure note the owner hid, per library: hidden until more tracks fail. */
const DISMISSED_KEY = "pm.bg.noteHidden.v1";

/** Waits this long after the last finished result before reading energy / sections again. */
const RELOAD_AFTER_MS = 1500;
const PEAKS_BATCH_MS = 250;

class Bg {
  settings = $state<BgSettings>({ ...DEFAULT_BG_SETTINGS });
  status = $state<BgStatus>({ ...NO_STATUS });
  failures = $state.raw<BgFailure[]>([]);
  /** The failure note was hidden when the list was this long, per library root. Kept across
   *  launches: the same unreadable file must not bring the note back at every start. */
  #dismissed = $state<Record<string, number>>(loadPref(DISMISSED_KEY, parseDismissed, () => ({})));
  get dismissedAt(): number {
    return this.#dismissed[this.status.root ?? ""] ?? 0;
  }
  set dismissedAt(n: number) {
    const root = this.status.root;
    if (!root) return;
    const next = { ...this.#dismissed };
    if (n > 0) next[root] = n;
    else delete next[root];
    this.#dismissed = next;
    savePref(DISMISSED_KEY, next);
  }
  /** Settings have been read from the backend. */
  loaded = $state(false);

  #reload: ReturnType<typeof setTimeout> | null = null;
  #wantEnergy = false;
  #wantSections = false;
  #peaks = new Set<string>();
  #peaksTimer: ReturnType<typeof setTimeout> | null = null;
  #lastFailures = -1;
  #lastState = "";
  /** The library the last status was about: the failure list and its "Hide" belong to it. */
  #root: string | null = null;

  /** Reads settings, status and the failure list from the backend. */
  async load() {
    try {
      const v = await bgGet();
      this.settings = v.settings;
      this.status = v.status;
      this.failures = v.failures;
      this.#lastFailures = v.status.failures;
      this.#lastState = v.status.state;
      this.loaded = true;
    } catch (e) {
      toast.show(`Could not read the background analysis settings: ${e}`, "bad");
    }
  }

  /** Saves a change to the settings; what the backend kept replaces the form's values. */
  async save(patch: Partial<BgSettings>): Promise<boolean> {
    const next = { ...$state.snapshot(this.settings), ...patch };
    const before = this.settings;
    this.settings = next;
    try {
      this.settings = await bgSaveSettings(next);
      // a new floor forgets or writes held-back records; the status count does not show that
      if (patch.key_min_confidence_pct !== undefined && patch.key_min_confidence_pct !== before.key_min_confidence_pct) {
        void this.refreshFailures();
        setTimeout(() => void this.refreshFailures(), 2000);
      }
      return true;
    } catch (e) {
      this.settings = before;
      toast.show(`Could not save the background analysis settings: ${e}`, "bad");
      return false;
    }
  }

  /** Tells the worker which library is open. Safe to call as often as the library changes. */
  watch(root: string | null) {
    bgWatch(root).catch(() => {});
  }

  /** Whether a track or mix preview is playing (the worker waits while it does, if set). */
  setPlaying(playing: boolean) {
    bgSetPlaying(playing).catch(() => {});
  }

  /** A Sync plan is on screen or being made: the worker waits (see `bgSetHold`). */
  setHold(hold: boolean) {
    bgSetHold(hold).catch(() => {});
  }

  async setPaused(paused: boolean) {
    try {
      await bgSetPaused(paused);
    } catch (e) {
      toast.show(String(e), "bad");
    }
  }

  /** "Run now": starts on the open library, a Pacemaker included. */
  /** When "check now" was last pressed, so a check that finds nothing still shows it happened. */
  checkedAt = $state(0);

  async runNow() {
    const root = session.library?.root;
    if (!root) return;
    try {
      await bgRunNow(root);
      this.checkedAt = Date.now();
      toast.show("Checking this library for anything missing. If nothing is, nothing runs.");
    } catch (e) {
      toast.show(String(e), "bad");
    }
  }

  async retryFailures() {
    try {
      await bgRetryFailures();
      this.failures = [];
      this.dismissedAt = 0;
      // the retry starts a run: the list is read again when the worker changes state (held-back and
      // no-result records are not in the status count), and once now for what was recorded at once
      this.#lastFailures = 0;
      void this.refreshFailures();
    } catch (e) {
      toast.show(String(e), "bad");
    }
  }

  dismissNote() {
    this.dismissedAt = failureCount(this.failures);
  }

  /** Forgets the failure list read for another library, so the next status reads it afresh. The
   *  "Hide" is kept: it is remembered per library root (`dismissedAt`), and resetting it here
   *  would bring the hidden note back on every launch, since the first status counts as a change. */
  private forgetFailures() {
    this.failures = [];
    this.#lastFailures = -1;
    this.#lastState = "";
  }

  onStatus(s: BgStatus) {
    this.status = s;
    if (libraryChanged(this.#root, s.root)) this.forgetFailures();
    this.#root = s.root;
    // the list is read again only when its count or the worker's state moved, not on every status tick
    if (failuresStale({ failures: this.#lastFailures, state: this.#lastState }, s)) {
      this.#lastFailures = s.failures;
      this.#lastState = s.state;
      void this.refreshFailures();
    }
  }

  private async refreshFailures() {
    try {
      this.failures = (await bgGet()).failures;
      if (failureCount(this.failures) < this.dismissedAt) this.dismissedAt = 0;
    } catch {
      /* the next status tick tries again */
    }
  }

  /** Shows finished work: only for the library that is open. */
  onChanged(c: BgChanged) {
    const lib = session.library;
    if (!lib || lib.root !== c.root) return;
    switch (c.kind) {
      case "keys":
        for (const [key, ids] of groupKeys(c.keys)) session.patchTracks(ids, { key });
        break;
      case "energy":
        this.#wantEnergy = true;
        this.scheduleReload();
        break;
      case "sections":
        this.#wantSections = true;
        this.scheduleReload();
        break;
      case "peaks":
        for (const l of c.locations) this.#peaks.add(l);
        this.#peaksTimer ??= setTimeout(() => this.flushPeaks(lib.root), PEAKS_BATCH_MS);
        break;
    }
  }

  private scheduleReload() {
    if (this.#reload) clearTimeout(this.#reload);
    this.#reload = setTimeout(() => {
      this.#reload = null;
      if (this.#wantEnergy) void energy.load();
      if (this.#wantSections) void sections.load();
      this.#wantEnergy = this.#wantSections = false;
    }, RELOAD_AFTER_MS);
  }

  private flushPeaks(root: string) {
    this.#peaksTimer = null;
    const locations = [...this.#peaks];
    this.#peaks.clear();
    // a row that was told "no waveform yet" asks again; rows that never asked are left alone
    for (const l of forgetPeaks(root, locations)) peakTick.bump(l);
  }
}

export const bg = new Bg();
