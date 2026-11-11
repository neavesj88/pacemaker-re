// Stems commands (src-tauri/src/commands/stems.rs): vocals / instrumental versions of tracks,
// made on this computer by HTDemucs through ONNX Runtime (crates/pm-stems). Design and the model
// licence analysis: notes/vocal-isolation-pc.md.
//
// The model and the runtime are downloaded only after the user accepts each one's licence notice;
// nothing is written to the device by these commands (the results go to a PC folder, adding them
// to the card is the ordinary add-tracks step).
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";

/** `job` of a stem run in `job-done`. */
export const STEMS_JOB = "stems";
/** `job` of a model / runtime download in `job-progress` (done / total in bytes) and `job-done`. */
export const STEMS_DOWNLOAD_JOB = "stems-download";

export type StemSettings = {
  /** "auto" | "cpu" | "gpu" */
  device: string;
  /** Intra-op threads for the CPU path; 0 = automatic. */
  threads: number;
  /** "mp3" | "ogg" | "wav" */
  format: string;
  mp3_kbps: number;
  /** Keep the stems sample-aligned with the original (cancels the MP3 encoder delay). */
  mp3_align: boolean;
  ogg_quality: number;
  /** Share of a segment that overlaps the next, 10 to 90. */
  overlap_percent: number;
  /** "sum" (drums + bass + other) | "minus" (original minus vocals) */
  instrumental: string;
  /** Tracks longer than this are refused; 0 = no limit. */
  max_minutes: number;
  vocals_suffix: string;
  instrumental_suffix: string;
  /** What to do when a result file already exists: "skip" | "replace". */
  existing: string;
  /** An onnxruntime library chosen by the user; empty = the downloaded one. */
  runtime_path: string;
  /** The folder of the last run. */
  last_folder: string;
};

export type Consent = { notice_version: number; accepted_at: number };

export type Component = {
  id: string;
  kind: "model" | "runtime";
  name: string;
  summary: string;
  url: string;
  page_url: string;
  sha256: string;
  size_bytes: number;
  file: string;
  licence_name: string;
  licence_status: "clear" | "unclear";
  licence_text: string;
  notice_version: number;
  /** False when there is no download for this system. */
  available_here: boolean;
  installed: boolean;
  installed_bytes: number;
  consent: Consent | null;
  /** The user accepted the CURRENT notice of this component. */
  consent_current: boolean;
};

export type RuntimeStatus = { source: "custom" | "downloaded" | null; path: string | null; problem: string | null };

export type StemsStatus = {
  components: Component[];
  runtime: RuntimeStatus;
  /** Everything needed to separate is in place. */
  ready: boolean;
  /** Why not, in words. */
  blocked: string | null;
  settings: StemSettings & { consents?: Record<string, Consent> };
  settings_warning: string | null;
  busy: boolean;
  data_dir: string;
};

export type PlanAction = "make" | "replace" | "partial" | "skip" | "refuse";

export type PlanItem = {
  track_id: number;
  label: string;
  minutes: number;
  source: string;
  vocals_path: string;
  instrumental_path: string;
  vocals_exists: boolean;
  instrumental_exists: boolean;
  write_vocals: boolean;
  write_instrumental: boolean;
  action: PlanAction;
  /** The consequence in words. */
  what: string;
  problem: string | null;
  segments: number;
};

export type Plan = {
  dest: string;
  format_label: string;
  items: PlanItem[];
  make: number;
  replace: number;
  partial: number;
  skip: number;
  refuse: number;
  minutes: number;
  /** Existing files a run would overwrite, by full path. */
  replaced_files: string[];
};

export type StemsProgress = {
  /** "checking-model" | "loading-model" | "separating" */
  phase: string;
  index: number;
  count: number;
  label: string;
  segment: number;
  segments: number;
  eta_secs: number | null;
  device: string;
};

export type WrittenItem = { track_id: number; label: string; vocals: string | null; instrumental: string | null; seconds: number };
export type NotDone = { label: string; error: string };

export type StemsResult = {
  dest: string;
  device: string;
  written: WrittenItem[];
  skipped: NotDone[];
  failed: NotDone[];
  cancelled: boolean;
  limited_samples: number;
};

/** The parts to download, their licence notices and state, the runtime in use, the settings. */
export const stemsStatus = () => invoke<StemsStatus>("stems_status");

/** Saves the settings (validated by the backend, which returns what it stored). */
export const stemsSettingsSave = (settings: StemSettings) => invoke<StemSettings>("stems_settings_save", { settings });

/** Starts the download of one component. `acceptNoticeVersion` must be the notice version the
 *  user was shown and accepted. Resolves when the job has started: progress is `job-progress`
 *  (job "stems-download"), the end `job-done`. */
export const stemsDownload = (id: string, acceptNoticeVersion: number) => invoke<void>("stems_download", { id, acceptNoticeVersion });

/** Deletes a downloaded component from the app's own data folder. */
export const stemsRemove = (id: string) => invoke<void>("stems_remove", { id });

/** Stops a running download or stem run (unfinished files are removed). */
export const stemsCancel = () => invoke<void>("stems_cancel");

/** What a run would do with these tracks into `dest`, item by item. Reads only. */
export const stemsPlan = (root: string, trackIds: number[], dest: string) => invoke<Plan>("stems_plan", { root, trackIds, dest });

/** Makes the stems. Resolves at once with the number of tracks that will run; `stems-progress`
 *  events follow, then `stems-result`, then `job-done` (job "stems"). `confirmReplace` must be
 *  true when the plan replaces existing files. */
export const stemsRun = (root: string, trackIds: number[], dest: string, confirmReplace: boolean) =>
  invoke<{ tracks: number }>("stems_run", { root, trackIds, dest, confirmReplace });

export const onStemsProgress = (cb: (e: StemsProgress) => void) => listen<StemsProgress>("stems-progress", (e) => cb(e.payload));
export const onStemsResult = (cb: (e: StemsResult) => void) => listen<StemsResult>("stems-result", (e) => cb(e.payload));

/** The native folder picker; null when cancelled. */
export async function pickFolder(title: string): Promise<string | null> {
  const sel = await open({ directory: true, multiple: false, title });
  return typeof sel === "string" ? sel : null;
}

/** The native file picker for an onnxruntime library; null when cancelled. */
export async function pickRuntimeLibrary(): Promise<string | null> {
  const sel = await open({ multiple: false, title: "Choose the onnxruntime library", filters: [{ name: "Shared library", extensions: ["dll", "so", "dylib"] }] });
  return typeof sel === "string" ? sel : null;
}
