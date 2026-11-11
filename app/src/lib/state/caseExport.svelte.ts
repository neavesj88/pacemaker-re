// Exporting cases to a folder: the remembered choices, the dialog's selection, the preview (the
// backend's own plan) and the running job. The choices are the app's own preferences, kept in
// its config dir (see src-tauri/src/commands/cases/export_prefs.rs), never on the device.
//
// The preview and the export are made by the same backend function, so what the dialog lists is
// what is written. Nothing here writes to the device.
import {
  CASES_EXPORT_JOB,
  type ExportPlan,
  type ExportPrefs,
  cancelExport,
  getExportPrefs,
  planExport,
  setExportPrefs,
  startExport,
} from "$lib/api/cases";
import type { DoneEvent, ProgressEvent } from "$lib/api/types";
import { afterQuickExport, prefsToSave, requestOf } from "./exportLogic";
import { session } from "./session.svelte";
import { toast } from "./toast.svelte";

export const DEFAULT_PREFS: ExportPrefs = {
  folder: null,
  format: "m3u8",
  relative_paths: false,
  copy_audio: false,
  audio_layout: "shared",
  audio_names: "tags",
  conflict: "skip",
  remember: true,
};

export type Stage = "setup" | "running" | "done";

class CaseExport {
  /** The choices the dialog edits (the saved ones until the user changes something). */
  prefs = $state<ExportPrefs>({ ...DEFAULT_PREFS });
  /** What is saved on disk now. */
  saved: ExportPrefs = { ...DEFAULT_PREFS };
  warning = $state<string | null>(null);
  loaded = $state(false);
  #loading: Promise<void> | null = null;

  /** The dialog is open. */
  open = $state(false);
  caseIds = $state<Set<number>>(new Set());
  smartIds = $state<Set<number>>(new Set());

  plan = $state<ExportPlan | null>(null);
  planError = $state("");
  planning = $state(false);
  #seq = 0;

  stage = $state<Stage>("setup");
  startError = $state("");
  done = $state(0);
  total = $state(0);
  current = $state("");
  failures = $state<{ label: string; error: string }[]>([]);
  result = $state<DoneEvent | null>(null);
  cancelling = $state(false);

  /** Reads the saved choices once; later calls wait for the first. */
  ensure(): Promise<void> {
    if (this.loaded) return Promise.resolve();
    this.#loading ??= getExportPrefs()
      .then((s) => {
        this.saved = s.prefs;
        this.prefs = { ...s.prefs };
        this.warning = s.warning;
      })
      .catch((e) => {
        this.warning = `The export choices could not be read: ${e}`;
      })
      .finally(() => {
        this.loaded = true;
        this.#loading = null;
      });
    return this.#loading;
  }

  async #save(p: ExportPrefs) {
    try {
      const s = await setExportPrefs(p);
      this.saved = s.prefs;
      this.warning = s.warning;
    } catch (e) {
      toast.show(`Could not save the export choices: ${e}`, "bad", 8000);
    }
  }

  /** Opens the dialog with these cases and smart cases ticked. */
  async show(sel: { caseIds: number[]; smartIds: number[] }) {
    await this.ensure();
    if (this.stage === "running") {
      this.open = true;
      return;
    }
    this.prefs = { ...this.saved };
    this.caseIds = new Set(sel.caseIds);
    this.smartIds = new Set(sel.smartIds);
    this.plan = null;
    this.planError = "";
    this.startError = "";
    this.stage = "setup";
    this.result = null;
    this.open = true;
  }

  close() {
    this.open = false;
    if (this.stage === "done") this.stage = "setup";
  }

  toggleCase(id: number, on: boolean) {
    const s = new Set(this.caseIds);
    if (on) s.add(id);
    else s.delete(id);
    this.caseIds = s;
  }

  toggleSmart(id: number, on: boolean) {
    const s = new Set(this.smartIds);
    if (on) s.add(id);
    else s.delete(id);
    this.smartIds = s;
  }

  setSelection(caseIds: number[], smartIds: number[]) {
    this.caseIds = new Set(caseIds);
    this.smartIds = new Set(smartIds);
  }

  get request() {
    return requestOf(this.prefs, this.caseIds, this.smartIds);
  }

  /** Recomputes the preview for the current choices. Answers that arrive late are dropped. */
  async refreshPlan() {
    const root = session.library?.root;
    const seq = ++this.#seq;
    const req = this.request;
    if (!root || req.folder === "" || (req.case_ids.length === 0 && req.smart_ids.length === 0)) {
      this.plan = null;
      this.planError = "";
      this.planning = false;
      return;
    }
    this.planning = true;
    try {
      const plan = await planExport(root, req);
      if (seq !== this.#seq) return;
      this.plan = plan;
      this.planError = "";
    } catch (e) {
      if (seq !== this.#seq) return;
      this.plan = null;
      this.planError = String(e);
    } finally {
      if (seq === this.#seq) this.planning = false;
    }
  }

  /** Starts the export the preview showed. */
  async start() {
    const root = session.library?.root;
    if (!root || this.stage === "running") return;
    this.startError = "";
    const req = this.request;
    this.done = 0;
    this.total = 0;
    this.current = "";
    this.failures = [];
    this.result = null;
    this.cancelling = false;
    // running first: a one-file export can finish (job-done) before the start call has returned
    this.stage = "running";
    try {
      const started = await startExport(root, req);
      if (this.stage === "running") this.total = started.writes;
      // the choices are saved once the export has really started
      await this.#save(prefsToSave($state.snapshot(this.prefs) as ExportPrefs, this.saved));
    } catch (e) {
      this.stage = "setup";
      this.startError = String(e);
    }
  }

  async cancel() {
    this.cancelling = true;
    try {
      await cancelExport();
    } catch (e) {
      this.cancelling = false;
      toast.show(`Could not cancel: ${e}`, "bad");
    }
  }

  /** `job-progress`, from views/cases/init.ts. */
  onProgress(ev: ProgressEvent) {
    if (ev.job !== CASES_EXPORT_JOB) return;
    if (ev.status === "failed") this.failures.push({ label: ev.label, error: ev.error ?? "failed" });
    if (ev.done > 0 || ev.status !== "failed") {
      this.done = Math.max(this.done, ev.done);
      this.current = ev.label;
    }
    if (ev.total > 0) this.total = ev.total;
  }

  /** `job-done`, from views/cases/init.ts. */
  onDone(ev: DoneEvent) {
    if (ev.job !== CASES_EXPORT_JOB) return;
    this.result = ev;
    this.cancelling = false;
    if (this.open) this.stage = "done";
    else {
      this.stage = "setup";
      toast.show(ev.error ? `Export failed: ${ev.error}` : ev.summary || "Export finished", ev.error || ev.failed > 0 ? "bad" : "ok", 10000);
    }
  }

  /** After a one-file export from a menu: the remembered folder and format follow it. */
  async rememberQuick(dest: string, format: ExportPrefs["format"]) {
    await this.ensure();
    const next = afterQuickExport(this.saved, dest, format);
    if (next) await this.#save(next);
  }
}

export const caseExport = new CaseExport();
