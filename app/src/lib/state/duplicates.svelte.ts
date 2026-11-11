// The duplicate finder: its settings, the groups found, the user's choices per group, the
// fingerprint job and the merge. Finding is read-only; the merge is shown as a plan (the
// backend's own, the one the job is made from) and runs only after the review dialog is
// confirmed. Settings are the app's own preferences, kept in its config dir, never on the device.
import {
  DUPES_FINGERPRINT_JOB,
  DUPES_MERGE_JOB,
  type DupSettings,
  type FindResult,
  type MergePlan,
  cancelFingerprints,
  findDuplicates,
  getDupSettings,
  planMerge,
  setDupSettings,
  startFingerprints,
  startMerge,
} from "$lib/api/duplicates";
import type { DoneEvent, ProgressEvent } from "$lib/api/types";
import { cases } from "./cases.svelte";
import { type Choice, defaultChoices, pickedCount, requestFor, withExcluded, withKeeper } from "./dupeLogic";
import { jobs } from "./jobs.svelte";
import { session } from "./session.svelte";
import { toast } from "./toast.svelte";

export const DEFAULT_SETTINGS: DupSettings = {
  by_tags: true,
  by_size: true,
  by_audio: false,
  strip_feat: true,
  ignore_brackets: false,
  ignore_the: true,
  tag_duration_secs: 10,
  size_duration_secs: 1,
  audio_threshold: 0.9,
  audio_duration_secs: 3,
  audio_offset_secs: 5,
  fingerprint_threads: 0,
  keeper_rule: "bitrate",
  move_cases: true,
  rating_rule: "max",
  plays_rule: "max",
};

export type Fingerprinting = {
  running: boolean;
  cancelling: boolean;
  done: number;
  total: number;
  current: string;
  failures: { label: string; error: string }[];
  summary: string;
};

class Duplicates {
  settings = $state<DupSettings>({ ...DEFAULT_SETTINGS });
  settingsWarning = $state<string | null>(null);
  settingsOpen = $state(false);
  #loaded: Promise<void> | null = null;

  result = $state.raw<FindResult | null>(null);
  /** The user's choice per group id: the copy to keep, tracks that are not copies, whether it is in the merge. */
  choices = $state<Record<number, Choice>>({});
  finding = $state(false);
  error = $state("");
  /** The settings changed since the groups were found. */
  stale = $state(false);

  fp = $state<Fingerprinting | null>(null);

  mergeOpen = $state(false);
  removeRows = $state(false);
  deleteFiles = $state(false);
  plan = $state.raw<MergePlan | null>(null);
  planError = $state("");
  planning = $state(false);
  starting = $state(false);
  #seq = 0;

  get picked(): number {
    return pickedCount(this.result, this.choices);
  }

  /** Reads the saved settings once. */
  ensureSettings(): Promise<void> {
    this.#loaded ??= getDupSettings()
      .then((s) => {
        this.settings = s.settings;
        this.settingsWarning = s.warning;
      })
      .catch((e) => {
        this.settingsWarning = `The duplicate settings could not be read: ${e}`;
      });
    return this.#loaded;
  }

  /** Saves the settings (what the dialog and the toolbar edit) and notes the groups are out of date. */
  async saveSettings(next: DupSettings) {
    this.settings = next;
    this.stale = this.result !== null;
    try {
      const s = await setDupSettings(next);
      this.settings = s.settings;
      this.settingsWarning = s.warning;
    } catch (e) {
      toast.show(`Could not save the duplicate settings: ${e}`, "bad", 8000);
    }
  }

  async find() {
    const root = session.library?.root;
    if (!root || this.finding) return;
    await this.ensureSettings();
    this.finding = true;
    this.error = "";
    try {
      const r = await findDuplicates(root, this.settings);
      if (session.library?.root !== root) return;
      // keep what the user already decided about a group that is still there
      const fresh = defaultChoices(r);
      for (const g of r.groups) {
        const old = this.choices[g.id];
        if (old && g.members.some((m) => m.track_id === old.keeper)) {
          fresh[g.id] = { keeper: old.keeper, excluded: old.excluded.filter((id) => id !== old.keeper && g.members.some((m) => m.track_id === id)), picked: old.picked };
        }
      }
      this.result = r;
      this.choices = fresh;
      this.stale = false;
    } catch (e) {
      this.error = String(e);
    } finally {
      this.finding = false;
    }
  }

  setKeeper(groupId: number, id: number) {
    const c = this.choices[groupId];
    if (c) this.choices = { ...this.choices, [groupId]: withKeeper(c, id) };
  }

  setExcluded(groupId: number, id: number, excluded: boolean) {
    const c = this.choices[groupId];
    if (c) this.choices = { ...this.choices, [groupId]: withExcluded(c, id, excluded) };
  }

  setPicked(groupId: number, picked: boolean) {
    const c = this.choices[groupId];
    if (c) this.choices = { ...this.choices, [groupId]: { ...c, picked } };
  }

  pickAll(picked: boolean) {
    this.choices = Object.fromEntries(Object.entries(this.choices).map(([k, c]) => [k, { ...c, picked }]));
  }

  // ---- the fingerprint job ---------------------------------------------------------------------

  async startFingerprinting() {
    const root = session.library?.root;
    if (!root || this.fp?.running) return;
    this.fp = { running: true, cancelling: false, done: 0, total: this.result?.pending.length ?? 0, current: "starting…", failures: [], summary: "" };
    try {
      const n = await startFingerprints(root, this.settings);
      if (this.fp) this.fp.total = n;
    } catch (e) {
      this.fp = null;
      this.error = String(e);
    }
  }

  async cancelFingerprinting() {
    if (!this.fp?.running) return;
    this.fp.cancelling = true;
    await cancelFingerprints().catch(() => {});
  }

  dismissFingerprint() {
    if (this.fp && !this.fp.running) this.fp = null;
  }

  onProgress(e: ProgressEvent) {
    if (e.job === DUPES_FINGERPRINT_JOB && this.fp) {
      this.fp.done = e.done;
      this.fp.total = e.total;
      this.fp.current = e.label;
      if (e.status === "failed") this.fp.failures.push({ label: e.label, error: e.error ?? "failed" });
    }
  }

  async onDone(e: DoneEvent) {
    if (e.job === DUPES_FINGERPRINT_JOB) {
      if (this.fp) {
        this.fp.running = false;
        this.fp.summary = e.error ?? e.summary;
      }
      await this.find();
    } else if (e.job === DUPES_MERGE_JOB) {
      toast.show(e.error ?? e.summary, e.error || e.failed ? "bad" : "ok", 8000);
      this.mergeOpen = false;
      this.plan = null;
      await cases.refresh();
      await this.find();
    }
  }

  // ---- the merge -----------------------------------------------------------------------------------

  /** Opens the review of the merge for the groups that are ticked. */
  openMerge() {
    if (!this.result || this.picked === 0 || !jobs.guard()) return;
    this.removeRows = false;
    this.deleteFiles = false;
    this.plan = null;
    this.planError = "";
    this.mergeOpen = true;
    void this.replan();
  }

  closeMerge() {
    this.mergeOpen = false;
    this.#seq++;
  }

  setRemoveRows(on: boolean) {
    this.removeRows = on;
    if (!on) this.deleteFiles = false;
    void this.replan();
  }

  setDeleteFiles(on: boolean) {
    this.deleteFiles = this.removeRows && on;
    void this.replan();
  }

  /** The plan for the choices as they stand: what the dialog lists and the job is started from. */
  async replan() {
    const root = session.library?.root;
    if (!root || !this.result) return;
    const seq = ++this.#seq;
    this.planning = true;
    this.planError = "";
    try {
      const plan = await planMerge(root, requestFor(this.result, this.choices, this.removeRows, this.deleteFiles, this.settings));
      if (seq === this.#seq) this.plan = plan;
    } catch (e) {
      if (seq === this.#seq) {
        this.plan = null;
        this.planError = String(e);
      }
    } finally {
      if (seq === this.#seq) this.planning = false;
    }
  }

  /** Runs the merge the dialog shows. Nothing is written if the library changed since the plan. */
  async confirmMerge() {
    const root = session.library?.root;
    const plan = this.plan;
    if (!root || !plan || !this.result || this.planning || this.starting || !jobs.guard()) return;
    const request = requestFor(this.result, this.choices, this.removeRows, this.deleteFiles, this.settings);
    this.starting = true;
    try {
      // the same strip the shared jobs use shows the progress and the library reloads when it ends
      jobs.job = {
        kind: DUPES_MERGE_JOB,
        runningLabel: "Merging duplicates",
        doneLabel: "Merge finished",
        running: true,
        total: plan.totals.groups,
        done: 0,
        current: "",
        bpm: null,
        failures: [],
        result: null,
      };
      await startMerge(root, request, plan.digest);
      this.mergeOpen = false;
    } catch (e) {
      jobs.job = null;
      this.planError = String(e);
      void this.replan(); // the preview was out of date: show the current one
    } finally {
      this.starting = false;
    }
  }

  reset() {
    this.result = null;
    this.choices = {};
    this.error = "";
    this.stale = false;
    this.fp = null;
    this.mergeOpen = false;
    this.plan = null;
    this.#seq++;
  }
}

export const duplicates = new Duplicates();
session.onReset(() => duplicates.reset());
