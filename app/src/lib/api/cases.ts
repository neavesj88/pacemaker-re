// Case commands (src-tauri/src/commands/cases.rs).
import { invoke } from "@tauri-apps/api/core";

/** Track ids in a case, in the order they were added. */
export const caseTracks = (root: string, caseId: number) => invoke<number[]>("case_tracks", { root, caseId });
