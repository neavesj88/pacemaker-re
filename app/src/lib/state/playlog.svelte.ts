// The play log: settings, and the snapshot taken each time a library is read. The log itself is
// an app-side file (never on the device); the History view's Play log section reads it back
// through lib/api/history.ts. `tick` goes up after every snapshot so that view refreshes.
import { untrack } from "svelte";
import {
  playlogAdopt,
  playlogAdoptLog,
  playlogRecord,
  playlogSetSettings,
  playlogSettings,
  playlogStartNew,
  type PlayLogSettings,
  type RecordReport,
} from "$lib/api/history";
import { session } from "./session.svelte";
import { toast } from "./toast.svelte";

export const DEFAULT_SETTINGS: PlayLogSettings = {
  enabled: true,
  retention_days: 730,
  session_gap_minutes: 240,
  max_events: 400_000,
  identity_min_present_pct: 50,
  ask_before_new_log: true,
};

class PlayLogStore {
  settings = $state<PlayLogSettings>({ ...DEFAULT_SETTINGS });
  loaded = $state(false);
  /** Bumped when a snapshot finishes (changed or not): the view re-reads the log. */
  tick = $state(0);
  /** The last snapshot's result, for the status line. */
  last = $state<RecordReport | null>(null);
  /** Roots we already told the user something about this run, so a reload does not repeat it. */
  #told = new Set<string>();

  async loadSettings() {
    try {
      this.settings = await playlogSettings();
    } catch (e) {
      // Keep the defaults on screen; the next save will say if the file cannot be written.
      session.error = `The play log settings could not be read: ${String(e)}`;
    } finally {
      this.loaded = true;
    }
  }

  /** Saves the settings; the stored (clamped) copy replaces ours. Throws the error text. */
  async save(next: PlayLogSettings) {
    this.settings = await playlogSetSettings(next);
    // The retention, the event cap and the identity check apply at a snapshot, so take one now
    // (when a library is open and logging is on) rather than wait for the next time it is read.
    const root = session.library?.root;
    if (root) await this.record(root);
  }

  /** "Keep this log and continue" for a library the log refused. Throws the error text. */
  async adopt(root: string) {
    try {
      const rep = await playlogAdopt(root);
      this.last = rep;
      toast.show(`Play log kept. ${rep.detail ?? ""}`.trim(), "ok", 8000);
    } finally {
      this.tick++;
    }
  }

  /** Starts a new log for a library whose card matches the log of another drive. Throws the error text. */
  async startNew(root: string) {
    try {
      this.last = await playlogStartNew(root);
    } finally {
      this.tick++;
    }
  }

  /** Continues the log of another drive (a file name from the overview's candidates) here, then
   *  takes a snapshot into it. Throws the error text. */
  async continueLog(root: string, file: string) {
    try {
      await playlogAdoptLog(root, file);
    } finally {
      this.tick++;
    }
    await this.record(root);
  }

  /** Takes a snapshot of the open library's counters. Never throws: a failure is a banner. */
  async record(root: string) {
    try {
      const rep = await playlogRecord(root);
      this.last = rep;
      this.#tell(root, rep);
    } catch (e) {
      session.error = `The play log could not be updated: ${String(e)}`;
    } finally {
      this.tick++;
    }
  }

  #tell(root: string, rep: RecordReport) {
    const once = (what: string) => {
      const k = `${root}|${what}`;
      if (this.#told.has(k)) return false;
      this.#told.add(k);
      return true;
    };
    if (rep.status === "started" && once("started")) {
      toast.show("Play log started for this library. Plays are worked out from changes after this point.", "ok", 6000);
    } else if (rep.status === "mismatch" && once("mismatch")) {
      toast.show("This library does not match its play log. Nothing was logged; see History, Play log.", "bad", 8000);
    } else if (rep.status === "candidate" && once("candidate")) {
      toast.show("A play log from another drive matches this card. Nothing is logged until you choose; see History, Play log.", "ok", 8000);
    } else if (rep.status === "recovered" && once("recovered")) {
      toast.show("The old play log could not be read, so it was kept aside and a new one started.", "bad", 8000);
    }
  }

  /** Starts watching the open library: a snapshot each time it is read (opened or reloaded in
   *  place after a save, so tracks added since the last snapshot are known before they are
   *  played). Returns the teardown. */
  start(): () => void {
    void this.loadSettings();
    return $effect.root(() => {
      $effect(() => {
        const lib = session.library; // a new object on every read
        if (lib) untrack(() => void this.record(lib.root));
      });
    });
  }
}

export const playlog = new PlayLogStore();
