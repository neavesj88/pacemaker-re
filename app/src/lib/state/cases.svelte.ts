// Cases: the case open in the Cases view and its track ids, the case list with metadata,
// favourites, and the smart cases. Every write goes through lib/api/cases.ts and is followed by
// a light refresh (the case list only, not the whole library) that also patches the case
// counts the sidebar reads from `session.library.cases`.
import {
  type CaseExportFormat,
  type CaseInfo,
  type CaseMeta,
  type MergeDone,
  type MergePlan,
  type MergeTarget,
  type SmartCase,
  type TransferDone,
  type TransferNewDone,
  type TransferPlan,
  addTracksToCase,
  caseTracks,
  createCase,
  deleteCase,
  deleteCases,
  type ConfirmedCase,
  deleteSmartCase,
  duplicateCase,
  exportCase,
  exportSmartCase,
  favouriteIds,
  listCases,
  listSmartCases,
  mergeCases,
  mergePreview,
  removeTracksFromCase,
  renameCase,
  saveSmartCase,
  setCaseMeta,
  setCaseTracks,
  setSmartCaseOnDevice,
  smartCaseCounts,
  toggleFavourite,
  transferPreview,
  transferToNewCase,
  transferTracks,
} from "$lib/api/cases";
import { pickSavePath } from "$lib/api/import";
import type { Collection } from "$lib/api/types";
import { baseName } from "$lib/format";
import { caseExport } from "./caseExport.svelte";
import { EXT, FORMAT_LABEL, saveSuggestion } from "./exportLogic";
import { jobs } from "./jobs.svelte";
import { session } from "./session.svelte";
import { caseMembers } from "./caseMembers.svelte";
import { referencedCases } from "./smartRules";
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
  /** Why a smart case cannot be evaluated right now (a rule names a case that was deleted), by id. */
  smartErrors = $state<Record<number, string>>({});
  /** The smart case being viewed: its id, 0 for a new unsaved one, null for none. */
  smartOpen = $state<number | null>(null);
  busy = $state(false);
  /** True once the case list has been read for the current library (false: still loading). */
  loaded = $state(false);
  /** Set by the smart case editor while it holds unsaved edits: asks before `proceed` runs. */
  leaveGuard: ((proceed: () => void) => void) | null = null;

  get openInfo(): CaseInfo | undefined {
    return this.open ? this.infos.find((c) => c.id === this.open!.id) : undefined;
  }

  #root(): string | null {
    return session.library?.root ?? null;
  }

  /** Runs `proceed` (leaving the editor), after the editor has asked about unsaved edits. */
  leave(proceed: () => void) {
    if (this.leaveGuard) this.leaveGuard(proceed);
    else proceed();
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
      this.loaded = true;
      this.favourites = new Set(favs);
      session.setCases(infos.map((c) => ({ id: c.id, name: c.name, play_time_secs: c.play_time_secs, track_count: c.track_count })));
      caseMembers.invalidate();
      if (this.open) {
        const now = session.library.cases.find((c) => c.id === this.open!.id);
        if (now) this.open = now;
        else this.close();
      }
    } catch (e) {
      session.error = String(e);
      this.loaded = true;
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

  // ---- organising ---------------------------------------------------------------------------

  /** A copy of a case under a new name (same tracks, order and metadata); returns its id. */
  duplicate(caseId: number, name: string) {
    return this.#write((root) => duplicateCase(root, caseId, name.trim()));
  }

  /** What a merge would add, computed by the same code the merge runs. */
  async previewMerge(sourceIds: number[], target: number | null): Promise<MergePlan | undefined> {
    const root = this.#root();
    if (!root) return undefined;
    try {
      return await mergePreview(root, sourceIds, target);
    } catch (e) {
      session.error = String(e);
      return undefined;
    }
  }

  /** Merges cases without repeats into a new or an existing case; optionally deletes the sources.
   *  The open case follows: reloaded if it received tracks, closed if it was deleted. */
  async merge(sourceIds: number[], to: MergeTarget, deleteSources: boolean): Promise<MergeDone | undefined> {
    const done = await this.#write((root) => mergeCases(root, sourceIds, to, deleteSources));
    if (done && this.open) await this.#reloadOpen();
    return done;
  }

  /** What copying / moving tracks between cases would do. */
  async previewTransfer(from: number | null, to: number, ids: number[], move: boolean): Promise<TransferPlan | undefined> {
    const root = this.#root();
    if (!root) return undefined;
    try {
      return await transferPreview(root, from, to, ids, move);
    } catch (e) {
      session.error = String(e);
      return undefined;
    }
  }

  /** Copies (or moves, taking them out of `from`) tracks into case `to`. */
  async transfer(from: number | null, to: number, ids: number[], move: boolean): Promise<TransferDone | undefined> {
    const done = await this.#write((root) => transferTracks(root, from, to, ids, move));
    if (done && this.open && (this.open.id === from || this.open.id === to)) await this.#reloadOpen();
    return done;
  }

  /** Copies / moves tracks into a case created by the same write (nothing is left behind if it
   *  fails); returns what was done, with the new case's id. */
  async transferToNew(from: number | null, name: string, ids: number[], move: boolean): Promise<TransferNewDone | undefined> {
    const done = await this.#write((root) => transferToNewCase(root, from, name.trim(), ids, move));
    if (done && this.open && this.open.id === from) await this.#reloadOpen();
    return done;
  }

  /** Deletes several cases in one transaction, each only while it is still the case that was
   *  confirmed (name and creation time); says which were kept. */
  async removeMany(list: ConfirmedCase[]) {
    return this.#write((root) => deleteCases(root, list));
  }

  /** Tracks dropped on a case (from a table): appended, repeats skipped; says what happened. */
  async dropTracks(caseId: number, name: string, ids: number[]) {
    const n = await this.addTracks(caseId, ids);
    if (n === undefined) return;
    const noun = (k: number) => `${k} track${k === 1 ? "" : "s"}`;
    toast.show(n === 0 ? `Already in “${name}”` : n < ids.length ? `Added ${noun(n)} to “${name}” (${ids.length - n} already there)` : `Added ${noun(n)} to “${name}”`, "ok");
  }

  /** Asks where to save, then writes the case as M3U8, CSV or rekordbox XML. */
  async exportOne(c: { id: number; name: string }, format: CaseExportFormat) {
    const root = this.#root();
    if (!root) return;
    try {
      await caseExport.ensure();
      const dest = await pickSavePath(saveSuggestion(caseExport.saved, c.name, EXT[format]), FORMAT_LABEL[format], EXT[format]);
      if (!dest) return;
      const r = await exportCase(root, c.id, format, dest);
      toast.show(`Saved ${r.tracks} track${r.tracks === 1 ? "" : "s"} to ${baseName(r.path)}`, "ok");
      void caseExport.rememberQuick(dest, format);
    } catch (e) {
      toast.show(`Could not export: ${e}`, "bad", 8000);
    }
  }

  /** Asks where to save, then writes a smart case as it matches right now (M3U8, CSV or rekordbox
   *  XML: rekordbox XML cannot hold rules, so it is an ordinary playlist of those tracks). */
  async exportSmart(s: { id: number; name: string }, format: CaseExportFormat) {
    const root = this.#root();
    if (!root) return;
    try {
      await caseExport.ensure();
      const dest = await pickSavePath(saveSuggestion(caseExport.saved, s.name, EXT[format], "smart case"), FORMAT_LABEL[format], EXT[format]);
      if (!dest) return;
      const r = await exportSmartCase(root, s.id, format, dest);
      toast.show(`Saved ${r.tracks} track${r.tracks === 1 ? "" : "s"} that “${s.name}” matches now to ${baseName(r.path)}`, "ok", 6000);
      void caseExport.rememberQuick(dest, format);
    } catch (e) {
      toast.show(`Could not export: ${e}`, "bad", 8000);
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

  #countSeq = 0;
  #countTimer: ReturnType<typeof setTimeout> | null = null;

  /** `refreshSmartCounts` after a pause in the changes that call for it: every tag edit moves the
   *  labels version, and each count run evaluates every smart case over the whole library. */
  scheduleSmartCounts(ms = 500) {
    if (this.#countTimer) clearTimeout(this.#countTimer);
    this.#countTimer = setTimeout(() => {
      this.#countTimer = null;
      void this.refreshSmartCounts();
    }, ms);
  }

  async refreshSmartCounts() {
    const root = this.#root();
    const seq = ++this.#countSeq;
    if (!root || this.smart.length === 0) {
      this.smartCounts = {};
      this.smartErrors = {};
      return;
    }
    try {
      const counts = await smartCaseCounts(root);
      // an older answer that arrives after a newer request never overwrites it
      if (this.#root() === root && seq === this.#countSeq) {
        this.smartCounts = Object.fromEntries(counts.filter((c) => !c.error).map((c) => [c.id, c.count]));
        this.smartErrors = Object.fromEntries(counts.filter((c) => c.error).map((c) => [c.id, c.error as string]));
      }
    } catch (e) {
      session.error = String(e);
    }
  }

  /** Names of the smart cases whose rules read any of these cases (deleting one breaks them until fixed). */
  smartCasesUsing(caseIds: number[]): string[] {
    const gone = new Set(caseIds);
    return this.smart.filter((s) => [...referencedCases(s.rules)].some((id) => gone.has(id))).map((s) => s.name);
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

  reset() {
    this.close();
    this.infos = [];
    this.loaded = false;
    this.favourites = new Set();
    this.smart = [];
    this.smartCounts = {};
    this.smartErrors = {};
  }
}

export const cases = new Cases();
session.onReset(() => cases.reset());
