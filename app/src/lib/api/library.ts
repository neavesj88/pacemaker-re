// Library / session commands (src-tauri/src/commands/library.rs).
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import type { LibraryView } from "./types";

/** The two settings of the library on this computer (src-tauri/src/commands/library/computer.rs). */
export type ComputerLibrarySettings = {
  /** "" = the default (Music folder + "Pacemaker Revival"). */
  path: string;
  open_at_start: boolean;
};

export type ComputerLibraryInfo = {
  settings: ComputerLibrarySettings;
  /** Where the library is, or will be made. */
  path: string;
  default_path: string;
  /** A library is there now. */
  exists: boolean;
  warning: string | null;
};

export const computerLibraryGet = () => invoke<ComputerLibraryInfo>("computer_library_get");
/** Saves the settings; the library at the old path is not moved or deleted. */
export const computerLibrarySet = (settings: ComputerLibrarySettings) => invoke<ComputerLibraryInfo>("computer_library_set", { settings });
/** Makes the library at the configured path when there is none; returns its root. Safe to repeat. */
export const computerLibraryCreate = () => invoke<string>("computer_library_create");

/** Mounted volumes that hold a Pacemaker library. */
export const listVolumes = () => invoke<string[]>("list_volumes");

/** Reads a library read-only. */
/** The licences of everything the app contains (bundled `THIRD-PARTY-NOTICES.md`). */
export const appNotices = () => invoke<string>("app_notices");
export const openLibrary = (root: string) => invoke<LibraryView>("open_library", { root });

/** Folder picker for the drive (the folder that contains .Pacemaker); null if cancelled. */
export async function chooseLibraryFolder(): Promise<string | null> {
  const dir = await open({ directory: true, title: "Choose the Pacemaker drive (the folder that contains .Pacemaker)" });
  return typeof dir === "string" ? dir : null;
}

/** Folder picker for the library on this computer; null if cancelled. */
export async function chooseComputerLibraryFolder(): Promise<string | null> {
  const dir = await open({ directory: true, title: "Choose the folder for the library on this computer" });
  return typeof dir === "string" ? dir : null;
}
