// Library / session commands (src-tauri/src/commands/library.rs).
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import type { LibraryView } from "./types";

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
