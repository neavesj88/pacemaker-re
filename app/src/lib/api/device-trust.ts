// Trust-pack commands of the device area (src-tauri/src/commands/device/{trust,undo}.rs):
// restore from a snapshot, verify a backup, Health card fixes, undo of rating / tag / favourite
// edits, and the settings of those. Re-exported from api/device.ts.
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";

export const JOB_VERIFY = "device-verify";
export const UNDO_CHANGED = "undo-changed";

/** What the user can tune in the trust pack. Kept in the app's config folder, never on the card. */
export type TrustSettings = {
  /** How many edits the undo history keeps per library. 0 turns the history off. */
  undo_limit: number;
  /** Percent of the library above which removing dead rows asks for an extra acknowledgement. */
  dead_row_guard_percent: number;
  /** How long the Undo pill stays after an edit, in seconds. 0 hides it. */
  undo_pill_seconds: number;
};
export type TrustSettingsView = { settings: TrustSettings; max_undo_limit: number; warning: string | null };

export const deviceTrustSettings = () => invoke<TrustSettingsView>("device_trust_settings");
export const deviceTrustSettingsSet = (settings: TrustSettings) =>
  invoke<TrustSettingsView>("device_trust_settings_set", { settings });

// -- restore from a snapshot

export type LibraryCounts = { tracks: number; cases: number; mixes: number };

export type DiffCounts = {
  /** In the snapshot, not on the card now: a restore brings them back. */
  tracks_returning: number;
  /** On the card now, not in the snapshot: a restore removes their rows. */
  tracks_lost: number;
  tracks_changed: number;
  cases_returning: number;
  cases_lost: number;
  cases_changed: number;
  mixes_returning: number;
  mixes_lost: number;
  mixes_changed: number;
};

export type SnapshotRow = {
  name: string;
  folder: string;
  /** UTC time from the folder name, "YYYYMMDDTHHMMSSZ" (or ""). */
  stamp: string;
  files: string[];
  music_db_bytes: number;
  /** Why it cannot be restored, when it cannot. */
  problem: string | null;
  snapshot: LibraryCounts | null;
  counts: DiffCounts | null;
};

export type SnapshotsPanel = { folder: string; keep: number; rows: SnapshotRow[] };

export type ItemRef = { id: number; label: string; note: string | null };
/** The first names of a list, and its real size. */
export type Listed = { total: number; items: ItemRef[] };

export type SnapshotDiff = {
  snapshot_name: string;
  snapshot: LibraryCounts;
  /** null when the library on the card cannot be read (see current_problem). */
  current: LibraryCounts | null;
  current_problem: string | null;
  counts: DiffCounts;
  tracks_returning: Listed;
  tracks_lost: Listed;
  tracks_changed: Listed;
  cases_returning: Listed;
  cases_lost: Listed;
  cases_changed: Listed;
  mixes_returning: Listed;
  mixes_lost: Listed;
  mixes_changed: Listed;
  /** Send this back to the restore: if the card or the snapshot changed meanwhile, it refuses. */
  fingerprint: string;
};

export type RestoreReport = {
  restored_from: string;
  /** The snapshot made of the card's database just before it was replaced. */
  safety_copy: string;
  counts: LibraryCounts;
};

export const deviceSnapshotRows = (root: string) => invoke<SnapshotsPanel>("device_snapshot_rows", { root });
export const deviceSnapshotPreview = (root: string, name: string) =>
  invoke<SnapshotDiff>("device_snapshot_preview", { root, name });
export const deviceRestoreSnapshot = (root: string, name: string, fingerprint: string) =>
  invoke<RestoreReport>("device_restore_snapshot", { root, name, fingerprint });

// -- verify a backup

export type CheckStatus = "ok" | "differs" | "missing" | "unreadable" | "unsafe" | "bad_line";

export type FileCheck = {
  path: string;
  status: CheckStatus;
  size: number | null;
  detail: string | null;
};

export type VerifyReport = {
  folder: string;
  listed: number;
  checked: number;
  ok: number;
  files: FileCheck[];
  /** Files in the folder that the manifest does not list. */
  extra: string[];
  bytes: number;
  cancelled: boolean;
};

/** Starts the job `device-verify`; progress and completion arrive as the shared job events. */
export const deviceVerifyBackup = (folder: string) => invoke<void>("device_verify_backup", { folder });
/** The per-file result of the last finished verification, or null. */
export const deviceVerifyReport = () => invoke<VerifyReport | null>("device_verify_report");

export async function pickVerifyFolder(): Promise<string | null> {
  const d = await open({ directory: true, title: "Choose a backup folder (pacemaker-backup-…) to verify" });
  return typeof d === "string" ? d : null;
}

// -- Health card fixes

export type DeadRow = {
  track_id: number;
  label: string;
  location: string;
  in_cases: string[];
  in_mixes: string[];
};
export type GridFix = { track_id: number; label: string; reason: string };
export type Unchecked = { track_id: number; label: string; location: string };

export type FixPlan = {
  /** false: .Pacemaker/Music is missing, so nothing can be judged dead. */
  music_folder_ok: boolean;
  total_tracks: number;
  /** Audio missing, in no mix: the rows a repair may remove. */
  dead_rows: DeadRow[];
  /** Audio missing but in a mix: kept. */
  kept_for_mixes: DeadRow[];
  unchecked: Unchecked[];
  /** Audio present, beat grid missing or unusable: re-analysis fixes these. */
  grids: GridFix[];
  guard_percent: number;
  /** More than guard_percent of the library looks dead: probably a card that is not fully mounted. */
  mass: boolean;
};

export type RemoveReport = { removed: string[]; skipped: string[]; cases_touched: number };

export const deviceFixPlan = (root: string) => invoke<FixPlan>("device_fix_plan", { root });
export const deviceFixRemoveRows = (root: string, trackIds: number[], acknowledgeMass: boolean) =>
  invoke<RemoveReport>("device_fix_remove_rows", { root, trackIds, acknowledgeMass });

// -- undo of rating, tag and favourite edits

export type EditKind = "rating" | "metadata" | "favourite";

export type UndoEntry = {
  id: number;
  kind: EditKind;
  /** Unix milliseconds. */
  at_ms: number;
  summary: string;
  tracks: number;
};

export type UndoView = {
  /** The library's volume key: undo-changed events carry it. */
  key: string;
  limit: number;
  /** Newest first. */
  entries: UndoEntry[];
};

export type PlanItem = {
  entry_id: number;
  track_id: number;
  label: string;
  /** "Rating", "Genre", ... or "Favourites". */
  what: string;
  current: string;
  becomes: string;
  /** Set when this line is left alone, and why. */
  skip: string | null;
};

export type UndoPlan = { entries: number; items: PlanItem[]; reverts: number; skipped: number };
export type UndoPreview = { top_entry_id: number; count: number; plan: UndoPlan };
export type UndoReport = { entries: number; reverted: number; skipped: number; items: PlanItem[] };

export type UndoChanged = {
  key: string;
  why: "recorded" | "undone" | "cleared" | "limit";
  summary: string | null;
  id: number;
};

export const deviceUndoList = (root: string) => invoke<UndoView>("device_undo_list", { root });
export const deviceUndoPreview = (root: string, count: number) =>
  invoke<UndoPreview>("device_undo_preview", { root, count });
export const deviceUndoApply = (root: string, count: number, topEntryId: number) =>
  invoke<UndoReport>("device_undo_apply", { root, count, topEntryId });
export const deviceUndoClear = (root: string) => invoke<void>("device_undo_clear", { root });
export const onUndoChanged = (cb: (e: UndoChanged) => void) => listen<UndoChanged>(UNDO_CHANGED, (e) => cb(e.payload));
