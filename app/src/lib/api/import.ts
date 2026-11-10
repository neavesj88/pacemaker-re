// Import commands (src-tauri/src/commands/import.rs) and the OS-level file pick / drop.
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open } from "@tauri-apps/plugin-dialog";

export const AUDIO_EXTS = ["mp3", "flac", "wav", "m4a", "aac", "ogg", "aiff"];

/** Files the user picks in the native dialog (empty if cancelled). */
export async function pickAudioFiles(): Promise<string[]> {
  const sel = await open({
    multiple: true,
    title: "Choose audio files to add",
    filters: [{ name: "Audio", extensions: AUDIO_EXTS }],
  });
  return Array.isArray(sel) ? sel : typeof sel === "string" ? [sel] : [];
}

/** The audio files among these paths; folders are searched recursively. */
export const expandAudioPaths = (paths: string[]) => invoke<string[]>("expand_audio_paths", { paths });

/** Analyse and add files to the library on `root`. Returns when the job has started. */
export const addTracks = (root: string, files: string[]) => invoke<void>("add_tracks", { root, files });

export type DragState = { type: "hover" } | { type: "leave" } | { type: "drop"; paths: string[] };

/** OS file drops arrive as Tauri webview events (not HTML5 drag events); absent in a plain
 *  browser, where the returned promise rejects (use it with `subscribeAll`). */
export const onFileDrag = (cb: (s: DragState) => void) =>
  getCurrentWebview().onDragDropEvent((e) => {
    const t = e.payload.type;
    if (t === "enter" || t === "over") cb({ type: "hover" });
    else if (t === "leave") cb({ type: "leave" });
    else if (t === "drop") cb({ type: "drop", paths: e.payload.paths });
  });
