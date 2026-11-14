// The device view's state: what was read from the volume, and the one running device job.
// Reads are plain commands; copying and deleting run as background jobs whose events arrive
// through init.ts.
import {
  deviceCancel,
  deviceEject,
  deviceHealth,
  deviceOverview,
  deviceSettings,
  deviceVerifyReport,
  JOB_RELINK,
  JOB_VERIFY,
  type DeviceOverview,
  type Health,
  type SettingsView,
  type VerifyReport,
} from "$lib/api/device";
import type { DoneEvent, ProgressEvent } from "$lib/api/types";
import { jobs } from "$lib/state/jobs.svelte";
import { nav } from "$lib/state/nav.svelte";
import { session } from "$lib/state/session.svelte";
import { toast } from "$lib/state/toast.svelte";
import { reloadPlan } from "./reloadLogic";

export type DeviceJob = { kind: string; label: string; done: number; total: number; detail: string | null };

class DeviceStore {
  overview = $state<DeviceOverview | null>(null);
  health = $state<Health | null>(null);
  /** undefined: not read yet; null: the device has no settings.dat yet. */
  settings = $state<SettingsView | null | undefined>(undefined);
  scanning = $state(false);
  job = $state<DeviceJob | null>(null);
  /** The finished line of the last job of each kind, kept on screen until the next one starts. */
  results = $state<Record<string, string>>({});
  cleanupOpen = $state(false);
  /** The Health card's "remove dead rows" dialog. */
  fixOpen = $state(false);
  /** The Health card's "find missing audio" (relink) dialog. */
  relinkOpen = $state(false);
  /** Bumped when the snapshot list may have changed (a restore), so the Restore card re-reads it. */
  snapshotTick = $state(0);
  /** The per-file result of the last finished backup verification. */
  verify = $state<VerifyReport | null>(null);
  verifyOpen = $state(false);
  #root = "";

  /** Anything that writes to the volume (a device job here, an import or re-analysis elsewhere,
   *  an eject) is running: eject, settings writes, installs, backups and clean-ups wait. */
  get blocked() {
    return !!this.job || jobs.busy || this.ejecting;
  }

  /** `lib` is the open library and its Pacemaker was ejected (the session marks it; opening a drive
   *  again gives a new object). */
  isDetached(lib: unknown) {
    return !!lib && lib === session.library && session.isDetached;
  }

  /** A different library was opened: nothing read for the old one is shown or reloaded. */
  reset() {
    this.overview = null;
    this.health = null;
    this.settings = undefined;
    this.results = {};
    this.verify = null;
    this.verifyOpen = false;
    this.#root = "";
  }

  cancel() {
    deviceCancel().catch((e) => toast.show(String(e), "bad", 8000));
  }

  /** Reads everything for a volume. Health scans every file, so it runs apart from the quick reads. */
  async load(root: string) {
    if (root !== this.#root) {
      this.overview = null;
      this.health = null;
      this.settings = undefined;
    }
    this.#root = root;
    // a library on this computer has no storage, firmware or settings.dat to read: only its health
    if (session.isComputer) return this.scan(root);
    const quick = Promise.all([
      deviceOverview(root).then((o) => (this.overview = o)),
      deviceSettings(root).then((s) => (this.settings = s)),
    ]).catch((e) => (session.error = String(e)));
    await Promise.all([quick, this.scan(root)]);
  }

  async scan(root: string) {
    this.scanning = true;
    try {
      this.health = await deviceHealth(root);
    } catch (e) {
      session.error = String(e);
    } finally {
      this.scanning = false;
    }
  }

  ejecting = $state(false);

  /** Windows shell eject. The library stays open in the app (it was read, not held), but the
   *  drive is gone: it is marked detached so every card is replaced by a notice, and the volume
   *  list is refreshed. */
  async eject(root: string) {
    const lib = session.library;
    this.ejecting = true;
    try {
      if (await deviceEject(root)) {
        session.markDetached(lib);
        this.overview = null;
        this.health = null;
        this.settings = undefined;
        this.results = {};
        this.verify = null;
        this.verifyOpen = false;
        toast.show("Ejected. It is safe to unplug the USB cable.", "ok", 8000);
        await session.refreshVolumes();
      } else {
        toast.show("Eject was requested but the drive is still there. Close anything using it and try again.", "bad", 8000);
      }
    } catch (e) {
      toast.show(String(e), "bad", 8000);
    } finally {
      this.ejecting = false;
    }
  }

  /** A job was accepted by the backend; show it as running until its done event. */
  begin(kind: string, label: string) {
    this.job = { kind, label, done: 0, total: 0, detail: null };
    delete this.results[kind];
  }

  onProgress(ev: ProgressEvent) {
    if (!ev.job.startsWith("device-")) return;
    this.job = { kind: ev.job, label: ev.label, done: ev.done, total: ev.total, detail: ev.detail };
  }

  async onDone(ev: DoneEvent) {
    if (!ev.job.startsWith("device-")) return;
    this.job = null;
    if (ev.error?.includes("Cancelled:")) {
      // stopped by the user: not a failure, and the backend has already cleaned up
      const msg = ev.error.slice(ev.error.indexOf("Cancelled:"));
      toast.show(msg, "ok", 8000);
      this.results[ev.job] = msg;
    } else if (ev.error) {
      toast.show(ev.error, "bad", 8000);
      this.results[ev.job] = `Failed: ${ev.error}`;
    } else {
      this.results[ev.job] = ev.summary;
      toast.show(ev.summary, ev.failed > 0 ? "bad" : "ok", 8000);
    }
    // only what the open library is: a job that ends after the user switched library or unplugged
    // the drive must not read the old root (a missing drive would show as an error after a success)
    const lib = session.library;
    const plan = reloadPlan(this.#root, lib?.root ?? null, this.isDetached(lib), nav.view === "device");
    if (plan === "forget") this.reset();
    if (ev.job === JOB_RELINK && lib && (plan === "load" || plan === "clear")) {
      // track locations changed: the open library and the Restore card's snapshot list are stale
      await session.load(lib.root, true);
      this.snapshotTick++;
    }
    if (ev.job === JOB_VERIFY) {
      // reads the backup folder only: the card did not change, so no new scan of it
      if (!ev.error) {
        try {
          this.verify = await deviceVerifyReport();
          // the per-file result is the point of the check: show it
          this.verifyOpen = !!this.verify;
        } catch (e) {
          toast.show(String(e), "bad", 8000);
        }
      }
      return;
    }
    if (plan === "load") await this.load(this.#root);
    else if (plan === "clear") {
      // not on screen: the view reads it afresh when it is opened, so no scan of a view nobody looks at
      this.overview = null;
      this.health = null;
      this.settings = undefined;
    }
  }
}

export const device = new DeviceStore();

session.onReset(() => device.reset());
