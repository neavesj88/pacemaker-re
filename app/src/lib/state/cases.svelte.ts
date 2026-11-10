// Cases: the case open in the Cases view and its track ids, the case list with metadata,
// favourites, and the smart cases. Every write goes through lib/api/cases.ts and is followed by
// a light refresh (the case list only, not the whole library) that also patches the case
// counts the sidebar reads from `session.library.cases`.
import {
  type CaseInfo,
  type CaseMeta,
  type SmartCase,
  addTracksToCase,
  caseTracks,
  createCase,
  deleteCase,
  deleteSmartCase,
  favouriteIds,
  listCases,
  listSmartCases,
  refreshSmartDeviceCases,
  removeTracksFromCase,
  renameCase,
  saveSmartCase,
  setCaseMeta,
  setCaseTracks,
  setSmartCaseOnDevice,
  smartCaseCounts,
  toggleFavourite,
} from "$lib/api/cases";
import type { Collection } from "$lib/api/types";
import { jobs } from "./jobs.svelte";
import { session } from "./session.svelte";
import { toast } from "./toast.svelte";

class Cases {
  /** The case being viewed, or null for the list. */
  open = $state<Collection | null>(null);
  /** Its track ids in case order (may include ids no longer in the library). */
  trackIds = $state<number[]>([]);
  /** Every case with its metadata, by name. */
  infos = $state<CaseInfo[]>([]);
  /** Ids of the tracks in the "Favourites" case. */
  favourites = $state<Set<number>>(new Set());

  smart = $state<SmartCase[]>([]);
  /** Match count per smart case id. */
  smartCounts = $state<Record<number, number>>({});
  /** The smart case being viewed: its id, 0 for a new unsaved one, null for none. */
  smartOpen = $state<number | null>(null);
  busy = $state(false);

  get openInfo(): CaseInfo | undefined {
    return this.open ? this.infos.find((c) => c.id === this.open!.id) : undefined;
  }

  #root(): string | null {
    return session.library?.root ?? null;
  }

  close() {
    this.open = null;
    this.trackIds = [];
    this.smartOpen = null;
  }

  /** Reads the case list (and favourites and smart counts) and patches the library's case
   *  counts. Cheap compared with reloading the library. */
  async refresh() {
    const root = this.#root();
    if (!root || !session.library) return;
    try {
      const [infos, favs] = await Promise.all([listCases(root), favouriteIds(root)]);
      if (session.library?.root !== root) return;
      this.infos = infos;
      this.favourites = new Set(favs);
      session.library.cases = infos.map((c) => ({ id: c.id, name: c.name, play_time_secs: c.play_time_secs, track_count: c.track_count }));
      if (this.open) {
        const now = session.library.cases.find((c) => c.id === this.open!.id);
        if (now) this.open = now;
        else this.close();
      }
    } catch (e) {
      session.error = String(e);
    }
    await this.refreshSmartCounts();
  }

  async openCase(c: Collection) {
    const root = this.#root();
    if (!root) return;
    try {
      this.trackIds = await caseTracks(root, c.id);
      this.open = c;
      this.smartOpen = null;
    } catch (e) {
      session.error = String(e);
    }
  }

  /** Runs a write after the same gate the jobs use; reports a failure in the red banner. */
  async #write<T>(fn: (root: string) => Promise<T>): Promise<T | undefined> {
    const root = this.#root();
    if (!root || !jobs.guard()) return undefined;
    this.busy = true;
    try {
      const out = await fn(root);
      await this.refresh();
      return out;
    } catch (e) {
      session.error = String(e);
      return undefined;
    } finally {
      this.busy = false;
    }
  }

  /** Creates a case (optionally with tracks) and returns its id. */
  create(name: string, trackIds: number[] = []) {
    return this.#write((root) => createCase(root, name.trim(), trackIds));
  }

  rename(caseId: number, name: string) {
    return this.#write((root) => renameCase(root, caseId, name.trim()));
  }

  saveMeta(caseId: number, meta: CaseMeta) {
    return this.#write((root) => setCaseMeta(root, caseId, meta));
  }

  async remove(caseId: number) {
    const done = await this.#write((root) => deleteCase(root, caseId));
    if (done && this.open?.id === caseId) this.close();
    return done;
  }

  /** Appends tracks to any case (the open one is reloaded); returns how many were new. */
  async addTracks(caseId: number, ids: number[]) {
    const n = await this.#write((root) => addTracksToCase(root, caseId, ids));
    if (n !== undefined && this.open?.id === caseId) await this.#reloadOpen();
    return n;
  }

  async removeTracks(ids: number[]) {
    const open = this.open;
    if (!open) return;
    const n = await this.#write((root) => removeTracksFromCase(root, open.id, ids));
    await this.#reloadOpen();
    return n;
  }

  /** Writes a new order. The list updates at once; if the write fails it is read back. */
  async reorder(ids: number[]) {
    const open = this.open;
    if (!open) return;
    const before = this.trackIds;
    this.trackIds = ids;
    const done = await this.#write(async (root) => (await setCaseTracks(root, open.id, ids), true));
    if (!done) {
      this.trackIds = before;
      await this.#reloadOpen();
    }
  }

  async #reloadOpen() {
    const root = this.#root();
    if (!root || !this.open) return;
    try {
      this.trackIds = await caseTracks(root, this.open.id);
    } catch (e) {
      session.error = String(e);
    }
  }

  /** The heart on a track: adds it to / removes it from the "Favourites" case. */
  async toggleFavourite(trackId: number) {
    const now = await this.#write((root) => toggleFavourite(root, trackId));
    if (now !== undefined && this.open) await this.#reloadOpen();
    return now;
  }

  // ---- smart cases ------------------------------------------------------------------------

  async loadSmart() {
    const root = this.#root();
    if (!root) return;
    try {
      const list = await listSmartCases(root);
      if (this.#root() === root) this.smart = list;
    } catch (e) {
      session.error = String(e);
    }
    await this.refreshSmartCounts();
  }

  async refreshSmartCounts() {
    const root = this.#root();
    if (!root || this.smart.length === 0) {
      this.smartCounts = {};
      return;
    }
    try {
      const counts = await smartCaseCounts(root);
      if (this.#root() === root) this.smartCounts = Object.fromEntries(counts.map((c) => [c.id, c.count]));
    } catch (e) {
      session.error = String(e);
    }
  }

  openSmart(id: number) {
    this.open = null;
    this.trackIds = [];
    this.smartOpen = id;
  }

  /** Saves rules in the app's config dir only; the device is not touched. Returns the stored one. */
  async saveSmart(sc: SmartCase): Promise<SmartCase | undefined> {
    const root = this.#root();
    if (!root) return undefined;
    try {
      const stored = await saveSmartCase(root, sc);
      await this.loadSmart();
      return stored;
    } catch (e) {
      session.error = String(e);
      return undefined;
    }
  }

  /** Deletes the smart case and, if asked, the case it made on the Pacemaker. */
  async deleteSmart(id: number, removeDeviceCase: boolean) {
    const root = this.#root();
    if (!root) return;
    if (removeDeviceCase && !jobs.guard()) return;
    try {
      await deleteSmartCase(root, id, removeDeviceCase);
      if (this.smartOpen === id) this.smartOpen = null;
      await this.loadSmart();
      if (removeDeviceCase) await this.refresh();
    } catch (e) {
      session.error = String(e);
    }
  }

  /** "Put on Pacemaker": on creates or refreshes the device case, off deletes it. */
  async setSmartOnDevice(id: number, on: boolean) {
    const copy = await this.#write((root) => setSmartCaseOnDevice(root, id, on));
    if (copy) {
      await this.loadSmart();
      toast.show(on ? `On the Pacemaker: ${copy.count} track${copy.count === 1 ? "" : "s"}` : "Removed from the Pacemaker", "ok");
    }
    return copy;
  }

  /** Re-runs every smart case that is on the Pacemaker and rewrites its device case. */
  async refreshDeviceCopies() {
    const done = await this.#write((root) => refreshSmartDeviceCases(root));
    if (done) {
      await this.loadSmart();
      toast.show(done.length === 0 ? "No smart case is on the Pacemaker" : `Refreshed ${done.length} smart case${done.length === 1 ? "" : "s"} on the Pacemaker`, "ok");
    }
    return done;
  }

  reset() {
    this.close();
    this.infos = [];
    this.favourites = new Set();
    this.smart = [];
    this.smartCounts = {};
  }
}

export const cases = new Cases();
session.onReset(() => cases.reset());
