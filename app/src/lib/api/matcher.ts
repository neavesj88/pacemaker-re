// Track matcher commands (src-tauri/src/commands/import/request.rs). The matching itself is a
// pure module in the UI (views/matcher/matchLogic.ts); these two move text across the file system.
import { invoke } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";

/** A request list read from disk. */
export type RequestFile = {
  text: string;
  bytes: number;
  /** "utf-8", "utf-16le", "utf-16be" or "latin-1" (the fallback for text that is not valid UTF-8). */
  encoding: string;
};

/** Reads a text, CSV, TSV or M3U/M3U8 file the user picked (at most 8 MB). Writes nothing. */
export const readRequestList = (path: string) => invoke<RequestFile>("matcher_read_list", { path });

/** Saves the missing-tracks list to `dest` (.txt or .csv) outside the library volume. */
export const saveMissingList = (root: string, dest: string, text: string) =>
  invoke<{ path: string; bytes: number }>("matcher_save_text", { root, dest, text });

/** The native file dialog for a request list (null if cancelled). */
export async function pickRequestFile(): Promise<string | null> {
  const sel = await open({
    multiple: false,
    title: "Choose a request list",
    filters: [
      { name: "Lists (text, CSV, playlist)", extensions: ["txt", "csv", "tsv", "m3u", "m3u8", "list"] },
      { name: "All files", extensions: ["*"] },
    ],
  });
  return typeof sel === "string" ? sel : null;
}

/** The native save dialog for the missing list (null if cancelled). */
export async function pickMissingSavePath(defaultName: string, ext: "txt" | "csv"): Promise<string | null> {
  const p = await save({
    title: "Save the missing tracks",
    defaultPath: defaultName,
    filters: [{ name: ext === "csv" ? "CSV list" : "Text list", extensions: [ext] }],
  });
  return typeof p === "string" ? p : null;
}
