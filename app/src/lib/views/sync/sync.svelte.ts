// The Sync view's state: the two libraries, the saved sync settings, the plan on screen, which
// items are left out, and the one running sync.
//
// The plan is only ever made by "Check what would change" and is dropped when anything it was made
// from changes (a side, a setting, a finished run). Apply sends that plan's digest and the left-out
// keys back, so the run is the plan the user saw or it does not run. The run is the write job
// "device-sync" on the target; its progress is shown by the device store's strip.
import { deviceCancel } from "$lib/api/device";
import {
  DEFAULT_SYNC_SETTINGS,
  JOB_SYNC,
  syncApply,
  syncPlan,
  syncSettingsGet,
  syncSettingsSet,
  type SyncPlan,
  type SyncSettings,
} from "$lib/api/sync";
import type { DoneEvent } from "$lib/api/types";
import { sameRoot } from "$lib/state/place";
import { session } from "$lib/state/session.svelte";
import { toast } from "$lib/state/toast.svelte";
import { device } from "../device/device.svelte";
import { excludedKeys, isStale, STALE_TEXT, type Pair } from "./syncLogic";

export type SyncResult = { summary: string; ok: number; failed: number; error: string | null };

class SyncStore {
  settings = $state<SyncSettings>({ ...DEFAULT_SYNC_SETTINGS });
  settingsLoaded = $state(false);
  settingsWarning = $state<string | null>(null);

  pair = $state<Pair>({ from: "", to: "" });
  /** The pair was chosen for this open session of the view (a deep link or a default), so a later
   *  visit keeps it instead of choosing again. */
  pairSet = $state(false);

  plan = $state.raw<SyncPlan | null>(null);
  /** The two roots the plan on screen was made for. */
  planFor = $state<Pair | null>(null);
  planning = $state(false);
  error = $state("");
  /** The backend refused the run because a library moved; the plan on screen is old. */
  stale = $state(false);
  excluded = $state<Set<string>>(new Set());

  starting = $state(false);
  running = $state(false);
  result = $state<SyncResult | null>(null);

  #gen = 0;
  #loading: Promise<void> | null = null;
  #saveTimer: ReturnType<typeof setTimeout> | undefined;
  /** The target of the run in flight, so the open library is read again when it finishes. */
  #target = "";

  /** Drops the plan and everything that hangs on it. The result of a finished run stays. */
  clearPlan() {
    this.#gen++;
    this.plan = null;
    this.planFor = null;
    this.planning = false;
    this.excluded = new Set();
    this.stale = false;
    this.error = "";
  }

  clear() {
    this.clearPlan();
    this.result = null;
    this.pairSet = false;
    this.pair = { from: "", to: "" };
  }

  setPair(p: Pair) {
    this.pair = p;
    this.pairSet = true;
    this.result = null;
    this.clearPlan();
  }

  /** Reads the saved settings once; every caller waits for the same read. */
  loadSettings(): Promise<void> {
    this.#loading ??= (async () => {
      try {
        const v = await syncSettingsGet();
        this.settings = v.settings;
        this.settingsWarning = v.warning;
      } catch (e) {
        this.settingsWarning = String(e);
      }
      this.settingsLoaded = true;
    })();
    return this.#loading;
  }

  /** Changes the settings (copy, edit, assign) and saves them shortly after. The plan on screen was
   *  made with the old ones, so it is dropped. A change made before the saved settings were read
   *  waits for them, so the read cannot overwrite it. */
  update(edit: (s: SyncSettings) => void) {
    void this.loadSettings().then(() => {
      const next = $state.snapshot(this.settings) as SyncSettings;
      edit(next);
      this.settings = next;
      this.clearPlan();
      clearTimeout(this.#saveTimer);
      this.#saveTimer = setTimeout(() => void this.save(), 400);
    });
  }

  /** Saves a pending change now, so the plan and the run read the settings that are on screen. */
  async flushSettings() {
    if (this.#saveTimer === undefined) return;
    clearTimeout(this.#saveTimer);
    this.#saveTimer = undefined;
    await this.save();
  }

  async save() {
    this.#saveTimer = undefined;
    try {
      const v = await syncSettingsSet($state.snapshot(this.settings) as SyncSettings);
      this.settingsWarning = v.warning;
    } catch (e) {
      this.error = String(e);
    }
  }

  resetSettings() {
    this.update((s) => Object.assign(s, DEFAULT_SYNC_SETTINGS));
  }

  /** "Check what would change": reads both libraries (read-only) and keeps the plan. */
  async check() {
    const { from, to } = this.pair;
    if (!from || !to || this.planning || this.busy) return;
    const gen = ++this.#gen;
    this.planning = true;
    this.error = "";
    this.stale = false;
    this.result = null;
    try {
      await this.flushSettings();
      const plan = await syncPlan(from, to);
      if (gen !== this.#gen) return; // the pair or a setting changed meanwhile
      this.plan = plan;
      this.planFor = { from, to };
      this.excluded = new Set();
    } catch (e) {
      if (gen !== this.#gen) return;
      this.plan = null;
      this.planFor = null;
      this.error = String(e);
    } finally {
      if (gen === this.#gen) this.planning = false;
    }
  }

  get busy() {
    return this.starting || this.running || device.blocked;
  }

  toggle(key: string) {
    const next = new Set(this.excluded);
    if (next.has(key)) next.delete(key);
    else next.add(key);
    this.excluded = next;
  }

  /** Ticks (include) or unticks (leave out) these keys together. */
  setMany(keys: string[], include: boolean) {
    const next = new Set(this.excluded);
    for (const k of keys) {
      if (include) next.delete(k);
      else next.add(k);
    }
    this.excluded = next;
  }

  /** Runs the plan on screen with the boxes as they are. */
  async apply() {
    const plan = this.plan;
    const pf = this.planFor;
    if (!plan || !pf || this.busy) return;
    this.starting = true;
    this.error = "";
    this.stale = false;
    this.result = null;
    this.#target = pf.to;
    try {
      await this.flushSettings();
      // the done event can beat the reply of a very short run, so the run is marked before the call
      this.running = true;
      device.begin(JOB_SYNC, "Starting the sync");
      await syncApply(pf.from, pf.to, plan.digest, excludedKeys(plan.items, this.excluded));
    } catch (e) {
      this.running = false;
      device.job = null;
      this.refused(String(e));
    } finally {
      this.starting = false;
    }
  }

  /** A start that failed or a run that ended in an error. */
  refused(msg: string) {
    if (isStale(msg)) {
      this.stale = true;
      this.error = STALE_TEXT;
    } else {
      this.error = msg;
    }
  }

  /** job-done for "device-sync". */
  async onDone(ev: DoneEvent) {
    if (ev.job !== JOB_SYNC) return;
    this.running = false;
    const cancelled = !!ev.error && ev.error.includes("Cancelled:");
    if (ev.error && !cancelled) {
      this.refused(ev.error);
      return;
    }
    this.result = { summary: cancelled ? ev.error!.slice(ev.error!.indexOf("Cancelled:")) : ev.summary, ok: ev.ok, failed: ev.failed, error: null };
    // what was shown is now old, whether it ran or was stopped half way
    this.clearPlan();
    const lib = session.library;
    if (lib && this.#target && sameRoot(lib.root, this.#target)) await session.load(lib.root, true);
    void session.refreshComputer();
  }

  cancel() {
    deviceCancel().catch((e) => toast.show(String(e), "bad", 8000));
  }
}

export const sync = new SyncStore();
session.onReset(() => sync.clear());
