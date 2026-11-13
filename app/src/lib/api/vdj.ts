// VirtualDJ list export (src-tauri/src/commands/export/vdj.rs): a case becomes a VirtualDJ list
// file, optionally with copies of the audio, in a folder outside the device. Nothing here writes
// to the Pacemaker. Argument keys are camelCase; fields inside an object argument keep their Rust
// (snake_case) names.
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";

/** The `job` name of the export in the `job-progress` / `job-done` events. */
export const VDJ_EXPORT_JOB = "virtualdj-export";

export type NameFiles = "artist_title" | "original";

/** The user's choices. Every field is a setting in the dialog. */
export type VdjOptions = {
  /** Copy the audio into the export folder and point the list at the copies. */
  copy_audio: boolean;
  name_files: NameFiles;
  /** List attribute `ordered`: VirtualDJ keeps the list's order. */
  ordered: boolean;
  /** List attribute `noDuplicates`. */
  no_duplicates: boolean;
  /** Copy mode: where the export folder will be when VirtualDJ opens it ("" = where it is written). */
  path_base: string;
};

export type VdjRequest = {
  root: string;
  case_id: number;
  /** Only these tracks of the case (case order kept). Empty = the whole case. */
  only_track_ids: number[];
  list_name: string;
  dest_dir: string;
  options: VdjOptions;
};

export type EntryStatus = "copy" | "reuse" | "reference" | "left_out";

export type PlanEntry = {
  track_id: number;
  label: string;
  status: EntryStatus;
  /** File name of the audio in the export ("" when left out). */
  file: string;
  /** The path written into the list ("" when left out). */
  path: string;
  size: number;
  /** Why the track is left out; "" otherwise. */
  reason: string;
};

export type VdjPlan = {
  list_name: string;
  folder: string;
  list_file: string;
  /** The list file is already there: the export refuses unless told to replace it. */
  list_exists: boolean;
  copy_audio: boolean;
  entries: PlanEntry[];
  copy_count: number;
  reuse_count: number;
  reference_count: number;
  left_out_count: number;
  copy_bytes: number;
  notes: string[];
};

/** What exporting would do, track by track. Read-only; rejects with a sentence when the request
 * cannot work (no name, destination missing or inside the device, no tracks). The export itself
 * plans again with the same function. */
export const vdjExportPlan = (request: VdjRequest) => invoke<VdjPlan>("vdj_export_plan", { request });

/** Starts the export as a background job and resolves at once. Progress arrives as `job-progress`
 * (job "virtualdj-export", one step per track) and the end as `job-done`. An existing list file is
 * replaced only when `replaceList` is true. */
export const vdjExportStart = (request: VdjRequest, replaceList: boolean) =>
  invoke<{ folder: string; list_file: string }>("vdj_export_start", { request, replaceList });

/** Stops the running export. Files already copied stay; no list is written. */
export const vdjExportCancel = () => invoke<void>("vdj_export_cancel");

/** The native folder picker (null if cancelled). */
export async function pickExportFolder(): Promise<string | null> {
  const sel = await open({ directory: true, multiple: false, title: "Choose the folder to export into (a USB stick, for example)" });
  return typeof sel === "string" ? sel : null;
}
