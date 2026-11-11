// Stems store: the status of the model download, the settings, the chosen tracks and folder, the
// plan the user reviews, and the running job. The stems area's own store (ARCHITECTURE.md); the
// events reach it from views/stems/init.ts.
//
// Nothing here writes to the device: results go to a folder on this computer. Putting them on the
// card is `jobs.askAdd(files)`, the ordinary add-tracks step, which asks first.
import {
  pickFolder,
  stemsCancel,
  stemsDownload,
  stemsPlan,
  stemsRemove,
  stemsRun,
  stemsSettingsSave,
  stemsStatus,
  STEMS_DOWNLOAD_JOB,
  STEMS_JOB,
  type Plan,
  type StemSettings,
  type StemsProgress,
  type StemsResult,
  type StemsStatus,
} from "$lib/api/stems";
import type { DoneEvent, ProgressEvent } from "$lib/api/types";
import { jobs } from "./jobs.svelte";
import { session } from "./session.svelte";
import { toast } from "./toast.svelte";

export type Download = { id: string; name: string; done: number; total: number; detail: string; cancelling: boolean };

class Stems {
  status = $state<StemsStatus | null>(null);
  /** Why the status could not be read. */
  statusError = $state("");
  /** The settings being edited; saved with `saveSettings`. */
  draft = $state<StemSettings | null>(null);

  trackIds = $state<number[]>([]);
  dest = $state("");

  /** The plan under review; cleared whenever tracks, folder or settings change. */
  plan = $state<Plan | null>(null);
  planError = $state("");
  planning = $state(false);

  running = $state(false);
  progress = $state<StemsProgress | null>(null);
  result = $state<StemsResult | null>(null);
  cancelling = $state(false);

  download = $state<Download | null>(null);

  /** Another job (add, analyse, export, backup...) holds the job lock. */
  busy = $derived(jobs.busy || this.running || this.download !== null || (this.status?.busy ?? false));

  constructor() {
    session.onReset(() => this.clearLibrary());
  }

  /** Per-library state; the model, the settings and the last folder stay. */
  clearLibrary() {
    this.trackIds = [];
    this.plan = null;
    this.planError = "";
    this.result = null;
  }

  async refresh() {
    try {
      const s = await stemsStatus();
      this.status = s;
      this.statusError = "";
      if (!this.draft) this.draft = { ...s.settings };
      if (!this.dest && s.settings.last_folder) this.dest = s.settings.last_folder;
    } catch (e) {
      this.statusError = String(e);
    }
  }

  // ---- inputs --------------------------------------------------------------------------------

  setTracks(ids: number[]) {
    this.trackIds = [...new Set(ids)];
    this.plan = null;
    this.planError = "";
  }

  toggle(id: number) {
    this.setTracks(this.trackIds.includes(id) ? this.trackIds.filter((x) => x !== id) : [...this.trackIds, id]);
  }

  setDest(path: string) {
    this.dest = path;
    this.plan = null;
    this.planError = "";
  }

  async chooseDest() {
    try {
      const p = await pickFolder("Folder for the vocal and instrumental files");
      if (p) this.setDest(p);
    } catch (e) {
      toast.show(`Could not open the folder picker: ${e}`, "bad");
    }
  }

  // ---- settings ------------------------------------------------------------------------------

  /** Saves the draft; returns true when the backend accepted it. A changed setting also changes
   *  what a plan would do, so the plan is dropped. */
  async saveSettings(): Promise<boolean> {
    if (!this.draft) return false;
    try {
      const saved = await stemsSettingsSave(this.draft);
      this.draft = { ...saved };
      this.plan = null;
      await this.refresh();
      return true;
    } catch (e) {
      toast.show(String(e), "bad", 8000);
      return false;
    }
  }

  resetDraft() {
    if (this.status) this.draft = { ...this.status.settings };
  }

  // ---- downloads -----------------------------------------------------------------------------

  /** `version` is the notice version the user accepted in the licence dialog. */
  async startDownload(id: string, version: number) {
    const c = this.status?.components.find((x) => x.id === id);
    if (!c) return;
    this.download = { id, name: c.name, done: 0, total: c.size_bytes, detail: "Starting…", cancelling: false };
    try {
      await stemsDownload(id, version);
    } catch (e) {
      this.download = null;
      toast.show(String(e), "bad", 10000);
    }
  }

  async cancelDownload() {
    if (!this.download) return;
    this.download.cancelling = true;
    await stemsCancel().catch(() => {});
  }

  async remove(id: string) {
    try {
      await stemsRemove(id);
      toast.show("Removed from this computer.", "ok");
    } catch (e) {
      toast.show(String(e), "bad", 8000);
    }
    await this.refresh();
  }

  // ---- plan and run --------------------------------------------------------------------------

  /** Works out what a run would do. The same backend function runs it later. */
  async review(): Promise<boolean> {
    const lib = session.library;
    if (!lib) return false;
    this.planError = "";
    this.planning = true;
    try {
      this.plan = await stemsPlan(lib.root, this.trackIds, this.dest.trim());
      return true;
    } catch (e) {
      this.plan = null;
      this.planError = String(e);
      return false;
    } finally {
      this.planning = false;
    }
  }

  async start(confirmReplace: boolean) {
    const lib = session.library;
    if (!lib || !this.plan) return;
    this.result = null;
    this.progress = null;
    this.cancelling = false;
    this.running = true;
    try {
      await stemsRun(lib.root, this.trackIds, this.plan.dest, confirmReplace);
      this.plan = null;
      void this.rememberFolder(this.dest.trim());
    } catch (e) {
      this.running = false;
      toast.show(String(e), "bad", 10000);
    }
  }

  /** Remembers the folder (a preference, not a policy) for next time. Saves the settings the run
   *  used (the saved ones), never the draft, so edits the user has not saved are not persisted. */
  private async rememberFolder(folder: string) {
    const saved = this.status?.settings;
    if (!saved || !folder || saved.last_folder === folder) return;
    try {
      await stemsSettingsSave({ ...saved, last_folder: folder });
      // last_folder is not an editable field, so keeping the draft in step does not touch its edits
      if (this.draft) this.draft.last_folder = folder;
      await this.refresh();
    } catch {
      // only a convenience; the run itself has started
    }
  }

  async cancel() {
    this.cancelling = true;
    await stemsCancel().catch(() => {});
  }

  /** The finished files, handed to the ordinary add-tracks step (which asks first). */
  async addResultsToCard(files: string[]) {
    await jobs.askAdd(files);
  }

  // ---- events (views/stems/init.ts) ----------------------------------------------------------

  onProgress(e: StemsProgress) {
    this.progress = e;
  }

  onResult(r: StemsResult) {
    this.result = r;
  }

  onJobProgress(e: ProgressEvent) {
    if (e.job === STEMS_DOWNLOAD_JOB && this.download) {
      this.download.done = e.done;
      this.download.total = e.total || this.download.total;
      this.download.detail = e.detail ?? "";
    }
  }

  async onJobDone(e: DoneEvent) {
    if (e.job === STEMS_DOWNLOAD_JOB) {
      this.download = null;
      toast.show(e.error ? `Download failed: ${e.error}` : e.summary, e.error ? "bad" : "ok", e.error ? 12000 : 6000);
      await this.refresh();
    } else if (e.job === STEMS_JOB) {
      this.running = false;
      this.cancelling = false;
      this.progress = null;
      if (e.error) toast.show(`Stems failed: ${e.error}`, "bad", 12000);
      else toast.show(e.summary || "Stems finished", e.failed > 0 ? "info" : "ok", 10000);
      await this.refresh();
    }
  }
}

export const stems = new Stems();
