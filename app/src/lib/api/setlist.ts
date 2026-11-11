// Printable setlist commands (src-tauri/src/commands/export/setlist.rs). The page itself is built
// in the UI (views/setlist/setlistLogic.ts); these move the user's notes and the finished file
// across the file system. Fields inside an object argument keep their Rust (snake_case) names.
import { invoke } from "@tauri-apps/api/core";
import { save } from "@tauri-apps/plugin-dialog";
import type { Sheet } from "$lib/views/setlist/setlistLogic";

/** A saved sheet; `warning` is set when the store file was damaged (saving then sets it aside). */
export type LoadedSheet = { sheet: Sheet; warning: string | null };

/** The header text, notes and energy ratings saved for a case (`case:<id>`) or mix (`mix:<id>`).
 * They live in the app's own config folder, never on the device. Reads only. */
export const loadSheet = (root: string, key: string) => invoke<LoadedSheet>("setlist_sheet_load", { root, key });

/** Saves them (a blank sheet removes the entry). Returns the sheet as stored, cleaned. */
export const saveSheet = (root: string, key: string, sheet: Sheet) => invoke<Sheet>("setlist_sheet_save", { root, key, sheet });

/** Writes the finished page to `dest`. Refuses a destination inside the library volume. */
export const saveSetlistHtml = (root: string, dest: string, html: string) =>
  invoke<{ path: string; bytes: number }>("setlist_save_html", { root, dest, html });

/** The native save dialog for the page (null if cancelled). */
export async function pickSetlistPath(defaultName: string): Promise<string | null> {
  const p = await save({ title: "Save the setlist page", defaultPath: defaultName, filters: [{ name: "Web page", extensions: ["html"] }] });
  if (typeof p !== "string") return null;
  // some platform dialogs return a bare name; the backend only accepts .html / .htm
  return /\.html?$/i.test(p) ? p : `${p}.html`;
}
