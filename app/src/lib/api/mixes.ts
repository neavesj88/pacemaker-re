// Mix commands (src-tauri/src/commands/mixes.rs).
import { invoke } from "@tauri-apps/api/core";

/** One mix as listed (the same shape as a case in the library listing). */
export type MixSummary = { id: number; name: string; play_time_secs: number; track_count: number };

/** The mixes on the volume. The mix editor reads this instead of `LibraryView.mixes`. */
export const mixList = (root: string) => invoke<MixSummary[]>("mix_list", { root });
