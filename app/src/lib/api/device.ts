// Device commands (src-tauri/src/commands/device.rs): storage, health, firmware file, backup,
// clean-up of unused files, settings.dat, eject. The long operations are background jobs whose
// kinds start with "device-"; their progress and completion arrive as the shared job events.
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";

export type Storage = {
  total_bytes: number;
  free_bytes: number;
  music_bytes: number;
  grids_mixes_bytes: number;
  other_bytes: number;
};

export type FirmwareInfo = {
  /** Only the last four characters are visible; the full serial never leaves the backend. */
  serial_masked: string | null;
  sw_version: string | null;
  fw_version: string | null;
  bootloader_version: string | null;
  editor_compatible_flag: string | null;
  /** .pfw files already waiting in .Pacemaker. */
  pending_pfw: string[];
};

export type DeviceOverview = { storage: Storage; firmware: FirmwareInfo };

export type UnreferencedFile = {
  /** Relative to .Pacemaker/Music, "/"-separated. */
  path: string;
  size: number;
  kind: "audio" | "grid" | "other";
};

export type MissingAudio = { track_id: number; location: string };

export type Health = {
  compatibility_problems: string[];
  missing_audio: MissingAudio[];
  unreferenced_checked: boolean;
  unreferenced: UnreferencedFile[];
  unreferenced_bytes: number;
  warnings: string[];
};

export type SettingView = {
  id: number;
  group: string;
  title: string;
  kind: "bool" | "list" | "float";
  /** bool: 0 or 1; list: the option index; float: the value. */
  value: number;
  default: number;
  /** bool: [label for off, label for on]; list: every option. */
  options: string[];
  editable: boolean;
  present: boolean;
  changed: boolean;
};

export type SettingsView = { settings: SettingView[]; other_items: number; total_items: number };

export type PfwInfo = {
  file_name: string;
  size: number;
  tag: string;
  sw_version: string;
  fw_version: string;
  bootloader_version: string;
  md5_ok: boolean;
  problems: string[];
  warnings: string[];
};

/** The Vocal filter flag on the card (`.Pacemaker/revival-vocal`): which direction the Wah pad
 * filters the vocals. Needs Revival firmware 180096073 or later; read by the device at boot. */
export type VocalMode = "off" | "vocal" | "inverted";

export type VocalState = {
  /** What the flag file says. */
  mode: VocalMode;
  /** `revival-no-preload` is on the card: the device plays its stock Wah whatever `mode` says. */
  kill_switch: boolean;
  /** What the device will play, and what mix previews and exports render: `mode`, or "off" under the kill switch. */
  effective: VocalMode;
};

export const JOB_FIRMWARE = "device-firmware";
export const JOB_BACKUP = "device-backup";
export const JOB_CLEANUP = "device-cleanup";

export const deviceOverview = (root: string) => invoke<DeviceOverview>("device_overview", { root });
export type SnapshotEntry = { name: string; folder: string; files: string[] };

/** The automatic safety copies of music.db (and settings.dat) the app makes before its first
 * write to a library in each session. They live in the app's data folder, never on the device. */
export type SnapshotsView = {
  folder: string;
  /** How many are kept per library. */
  keep: number;
  /** Newest first. */
  snapshots: SnapshotEntry[];
};

export const deviceSnapshots = (root: string) => invoke<SnapshotsView>("device_snapshots", { root });
/** Opens the snapshot folder in the file manager. */
export const deviceSnapshotsOpen = (root: string) => invoke<void>("device_snapshots_open", { root });
export const deviceHealth = (root: string) => invoke<Health>("device_health", { root });
/** null when the device has not written settings.dat yet. */
export const deviceSettings = (root: string) => invoke<SettingsView | null>("device_settings", { root });
/** Returns how many settings actually changed. */
export const deviceSettingsWrite = (root: string, changes: { id: number; value: number }[]) =>
  invoke<number>("device_settings_write", { root, changes });
/** Reads the Vocal filter flag (and the kill switch) from the card. */
export const deviceVocal = (root: string) => invoke<VocalState>("device_vocal", { root });
/** Sets the flag: off deletes revival-vocal, vocal writes it empty, inverted writes "i". Returns the state read back. */
export const deviceVocalSet = (root: string, mode: VocalMode) => invoke<VocalState>("device_vocal_set", { root, mode });
export const deviceFirmwareInspect = (path: string) => invoke<PfwInfo>("device_firmware_inspect", { path });
export const deviceFirmwareInstall = (root: string, path: string) =>
  invoke<void>("device_firmware_install", { root, path });
export const deviceBackup = (root: string, dest: string, includeAudio: boolean) =>
  invoke<void>("device_backup", { root, dest, includeAudio });
/** Asks the running device job (firmware copy, backup, clean-up) to stop at the next file. */
export const deviceCancel = () => invoke<void>("device_cancel");
export const deviceCleanup = (root: string, files: string[]) => invoke<void>("device_cleanup", { root, files });
/** True once the library has disappeared from the volume. */
export const deviceEject = (root: string) => invoke<boolean>("device_eject", { root });

export async function pickFirmwareFile(): Promise<string | null> {
  const f = await open({
    title: "Choose a Pacemaker firmware file",
    filters: [{ name: "Pacemaker firmware", extensions: ["pfw"] }],
  });
  return typeof f === "string" ? f : null;
}

export async function pickBackupFolder(): Promise<string | null> {
  const d = await open({ directory: true, title: "Choose where to save the backup (not on the Pacemaker)" });
  return typeof d === "string" ? d : null;
}
export * from "./device-trust";
export * from "./device-space";
