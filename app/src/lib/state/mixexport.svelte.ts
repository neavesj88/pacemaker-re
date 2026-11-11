// Audio export settings and loudness analysis of the open mix: the export dialog's state. The
// choices are per-viewer conveniences kept in this browser's storage (like the track list
// settings): they describe how the file is rendered, not anything on the device, and the dialog
// says so. The pure parts are in exportprefs.ts.
//
// A loudness analysis (`export_loudness_analyse`) answers one question: this mix, this range, these
// settings. `current` is true only while all three are unchanged, and only then does the preview
// play with its gains, so what is heard is what an export with the same settings writes.
import {
  exportLoudnessAnalyse,
  exportRangeResolve,
  type ExportExtras,
  type LevelSettings,
  type LoudnessReport,
  type PreviewGains,
  type RangeInfo,
  type RangePrefs,
  type RangeSpec,
} from "$lib/api/export";
import type { MixDoc } from "$lib/api/mixes";
import {
  analysisKey,
  defaultPrefs,
  encodingOf,
  levelsActive,
  levelsOf,
  mergePrefs,
  parseClock,
  previewGainsOf,
  problemWith,
  rangePrefsOf,
  rangeSpecOf,
  tagsOf,
  type ExportPrefs,
  type RangeMode,
} from "./exportprefs";
import { mixes } from "./mixes.svelte";
import { sameAudio } from "./sameaudio";
import { session } from "./session.svelte";
import { toast } from "./toast.svelte";
import { tracklist } from "./tracklist.svelte";

const STORAGE_KEY = "pacemaker-revival.mixexport";

function load(): ExportPrefs {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    return mergePrefs(raw ? JSON.parse(raw) : null);
  } catch {
    return defaultPrefs();
  }
}

const errText = (e: unknown) => (typeof e === "string" ? e : e instanceof Error ? e.message : String(e));

class MixExport {
  /** The export dialog is open. */
  open = $state(false);
  prefs = $state<ExportPrefs>(defaultPrefs());

  // what to export (per mix; not remembered)
  mode = $state<RangeMode>("whole");
  /** The time range as the user typed it. */
  fromText = $state("");
  toText = $state("");
  clipA = $state("");
  clipB = $state("");
  /** What the range covers, as the backend resolves it; `infoError` says why it cannot. */
  info = $state.raw<RangeInfo | null>(null);
  infoError = $state("");
  /** The title tag as typed for this mix; empty = the mix name. */
  titleText = $state("");

  /** The last analysis of this mix, and whether it is still about the current mix and settings. */
  report = $state.raw<LoudnessReport | null>(null);
  /** The last finished export's own measurement. */
  lastExport = $state.raw<LoudnessReport | null>(null);
  analysing = $state(false);
  analysePct = $state(0);

  #loaded = false;
  #reportDoc: MixDoc | null = null;
  #reportKey = "";
  #pending: { doc: MixDoc | null; key: string } | null = null;
  #infoSeq = 0;
  #forDoc = -1;

  /** Reads the saved settings once (storage can be missing or blocked: the defaults then apply). */
  ensureLoaded() {
    if (this.#loaded) return;
    this.#loaded = true;
    this.prefs = load();
  }

  save() {
    try {
      localStorage.setItem(STORAGE_KEY, JSON.stringify(this.prefs));
    } catch {
      // private window, blocked or full storage: the settings still apply, they are just forgotten
    }
  }

  /** A fresh dialog for a mix that was not the last one: the whole mix, nothing carried over. */
  startFor(docId: number | undefined) {
    if (docId === undefined || docId === this.#forDoc) return;
    this.#forDoc = docId;
    this.mode = "whole";
    this.fromText = "";
    this.toText = "";
    this.clipA = "";
    this.clipB = "";
    this.titleText = "";
    this.info = null;
    this.infoError = "";
    this.report = null;
    this.lastExport = null;
    this.#reportDoc = null;
  }

  // ---- the request ----------------------------------------------------------------------------

  get from(): number | null {
    return parseClock(this.fromText);
  }
  get to(): number | null {
    return parseClock(this.toText);
  }

  spec(): RangeSpec | null {
    return rangeSpecOf(this.mode, this.from, this.to, this.clipA, this.clipB);
  }
  levels(): LevelSettings {
    return levelsOf(this.prefs);
  }
  rangePrefs(): RangePrefs {
    return rangePrefsOf(this.prefs);
  }
  /** A sentence about a setting that is out of range, or null. */
  get problem(): string | null {
    return problemWith(this.prefs);
  }
  get active() {
    return levelsActive(this.levels());
  }

  /** The optional parts of `exportMixAudio` for the current choices, or null while they are not
   * a request (a time that does not parse). */
  extras(): ExportExtras | null {
    const range = this.spec();
    if (!range) return null;
    tracklist.ensureLoaded();
    return {
      range,
      rangePrefs: this.rangePrefs(),
      levels: this.levels(),
      encoding: encodingOf(this.prefs),
      // the chapters are the track list as the user has set it up for the track list dialog
      tags: tagsOf(this.prefs, this.titleText, tracklist.snapshot()),
    };
  }

  /** Is the export a part of the mix rather than all of it? */
  get partial() {
    return this.mode !== "whole";
  }

  /** Asks the backend what the chosen range covers (the numbers the export will use). */
  async refreshInfo() {
    const doc = mixes.doc;
    const range = this.spec();
    const seq = ++this.#infoSeq;
    if (!doc) return;
    if (!range) {
      this.info = null;
      this.infoError = this.mode === "time" ? "Enter the start and end as times, like 1:30 or 12:05.5." : "Choose the first and the last track.";
      return;
    }
    try {
      const info = await exportRangeResolve(doc, range, this.rangePrefs());
      if (seq !== this.#infoSeq) return;
      this.info = info;
      this.infoError = "";
    } catch (e) {
      if (seq !== this.#infoSeq) return;
      this.info = null;
      this.infoError = errText(e);
    }
  }

  // ---- the analysis ---------------------------------------------------------------------------

  #key(): string | null {
    const range = this.spec();
    return range ? analysisKey(this.levels(), range, this.rangePrefs()) : null;
  }

  /** The analysis is about the mix and settings in front of the user. */
  get current(): boolean {
    const key = this.#key();
    return this.report !== null && key !== null && this.#reportKey === key && sameAudio(this.#reportDoc, mixes.doc);
  }

  /** Starts a measuring job for the current choices. The report arrives through `receive`. */
  async analyse() {
    const root = mixes.root;
    const doc = mixes.doc;
    const extras = this.extras();
    if (!root || !doc || !extras || this.analysing) return;
    if (this.problem) {
      toast.show(this.problem, "bad", 8000);
      return;
    }
    this.#pending = { doc, key: this.#key() ?? "" };
    this.analysing = true;
    this.analysePct = 0;
    try {
      await exportLoudnessAnalyse(root, doc, extras.range, extras.rangePrefs, extras.levels);
    } catch (e) {
      this.analysing = false;
      this.#pending = null;
      toast.show(`Could not measure: ${errText(e)}`, "bad", 10000);
    }
  }

  /** A `mix-loudness-report` event. An analysis becomes the current one (for the mix and settings
   * it was started with); an export's report is kept to show what the file measured. */
  receive(r: LoudnessReport) {
    if (r.source === "export") {
      this.lastExport = r;
      return;
    }
    const p = this.#pending;
    this.#pending = null;
    this.#reportDoc = p?.doc ?? mixes.doc;
    this.#reportKey = p?.key ?? this.#key() ?? "";
    this.report = r;
  }

  /** The `job-done` of a measuring job. */
  analysisEnded(error: string | null, summary: string) {
    this.analysing = false;
    this.#pending = null;
    if (error) toast.show(`Measuring failed: ${error}`, "bad", 10000);
    else if (/cancelled/i.test(summary)) toast.show(summary, "info", 4000);
  }

  /** The gains the editor's preview plays with, or undefined (see `previewGainsOf`). */
  previewGains(): PreviewGains | undefined {
    return previewGainsOf(this.report, this.current, this.prefs.previewAtExportLevel);
  }

  /** Identifies the gains a preview was rendered with (empty for none). */
  gainsKey(): string {
    const g = this.previewGains();
    return g ? JSON.stringify(g) : "";
  }

  /** Settings that change the audio are on but the analysis does not match them: the preview
   * plays without the gains. */
  get previewBehind(): boolean {
    return this.prefs.previewAtExportLevel && this.active && !this.current;
  }

  clear() {
    this.report = null;
    this.lastExport = null;
    this.#reportDoc = null;
    this.#pending = null;
    this.analysing = false;
    this.info = null;
    this.infoError = "";
    this.mode = "whole";
    this.open = false;
    this.#forDoc = -1;
  }
}

export const mixExport = new MixExport();

session.onReset(() => mixExport.clear());
