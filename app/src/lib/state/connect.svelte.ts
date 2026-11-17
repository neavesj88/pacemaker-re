// The "Pacemaker connected" prompt: every few seconds the app lists the connected volumes (the same
// `list_volumes` the Rescan button runs: it only looks which drives are there) and, when a Pacemaker
// has appeared, offers to open it. A Pacemaker is never opened or read by this code on its own:
// `open` runs only from the user's click (see `Session.refreshVolumes`). The rules of what is new
// are in connectLogic.ts.
import { listVolumes } from "$lib/api/library";
import {
  CONNECT_ASK_DEFAULT,
  CONNECT_ASK_KEY,
  CONNECT_SECS_DEFAULT,
  CONNECT_SECS_KEY,
  EMPTY_STATE,
  answered,
  dismiss,
  normaliseAsk,
  normaliseSecs,
  step,
  type ConnectState,
} from "./connectLogic";
import { nav } from "./nav.svelte";
import { libraryKind, sameRoot } from "./place";
import { loadPref, savePref } from "./prefStore";
import { session } from "./session.svelte";
import { device } from "$lib/views/device/device.svelte";

class Connect {
  /** Ask to open a Pacemaker when one is connected (Settings > Library). */
  ask = $state(loadPref(CONNECT_ASK_KEY, normaliseAsk, () => CONNECT_ASK_DEFAULT));
  /** Seconds between checks; 0 = no checking. */
  secs = $state(loadPref(CONNECT_SECS_KEY, normaliseSecs, () => CONNECT_SECS_DEFAULT));
  /** The Pacemaker being opened after "Open it" (the buttons wait). */
  busy = $state(false);
  #s = $state.raw<ConnectState>(EMPTY_STATE);
  /** Restarts the wait with the current interval (set while checking runs). */
  #rearm: (() => void) | null = null;

  /** The drive the prompt is about; null when there is nothing to ask. */
  get shown(): string | null {
    return this.ask ? (this.#s.pending[0] ?? null) : null;
  }

  setAsk(on: boolean) {
    this.ask = on;
    savePref(CONNECT_ASK_KEY, on);
    // turned off: a question on screen goes; turned on: only drives plugged in from now on are new
    if (!on) this.#s = { ...this.#s, pending: [] };
  }

  /** Returns the number kept (whole seconds, 0 = off) so the field can show it. */
  setSecs(n: number): number {
    this.secs = normaliseSecs(n);
    savePref(CONNECT_SECS_KEY, this.secs);
    this.#rearm?.(); // the new interval counts now, not after the old wait runs out
    return this.secs;
  }

  /** One look at the connected volumes. Also keeps the header's list of places current. */
  async check() {
    let list: string[];
    try {
      list = await listVolumes();
    } catch {
      return; // no answer this time: nothing changes, the next check tries again
    }
    session.setVolumes(list);
    // the open Pacemaker gone without Eject: read-only from now on, a running write is stopped
    const open = session.attached;
    if (open && open.kind === "pacemaker" && libraryKind(open.root) === "pacemaker" && !list.some((v) => sameRoot(v, open.root))) {
      const writing = device.job ? device.job.label : null;
      if (device.job) device.cancel();
      session.markRemoved(writing);
    }
    this.#s = step(this.#s, list, session.attached?.root ?? null, this.ask);
  }

  /** "Not now" (and the checkbox): closes the question until the drive is unplugged and plugged in again. */
  notNow(volume: string, never: boolean) {
    if (never) this.setAsk(false);
    this.#s = dismiss(this.#s, volume);
  }

  /** "Open it": the same switch as choosing the Pacemaker in the header's picker (including the
   *  question about unsaved work), then the Device view. */
  async open(volume: string, never: boolean) {
    if (this.busy) return;
    if (never) this.setAsk(false);
    this.busy = true;
    try {
      await session.load(volume);
      if (session.attached && sameRoot(session.attached.root, volume)) nav.go("device", {}, true);
    } finally {
      this.#s = answered(this.#s, volume);
      this.busy = false;
    }
  }

  /** Starts the checking for the life of the app; returns the stop. A check never overlaps the
   *  next (a sleeping drive can make the listing slow). With checking off it only waits to be
   *  switched on, and then takes a fresh baseline, so a drive plugged in meanwhile is not "new". */
  start(): () => void {
    let stopped = false;
    let running = false;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const wait = () => {
      clearTimeout(timer);
      timer = setTimeout(tick, Math.max(1, this.secs) * 1000);
    };
    const tick = async () => {
      if (stopped) return;
      running = true;
      try {
        if (this.secs > 0) await this.check();
        else if (this.#s.known) this.#s = EMPTY_STATE;
      } finally {
        running = false;
      }
      if (!stopped) wait();
    };
    // a check in flight schedules the next wait itself, with whatever interval is set by then
    this.#rearm = () => {
      if (!stopped && !running) wait();
    };
    wait();
    return () => {
      stopped = true;
      this.#rearm = null;
      clearTimeout(timer);
    };
  }
}

export const connect = new Connect();
