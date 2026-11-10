// Mock of src-tauri/src/commands/mixes.rs. The mix fixture lives here (mixes-owned); library.ts
// reads it for the sidebar count. Invented placeholder data only.
import type { Handlers } from "./types";

export const mixes = [{ id: 1, name: "Example Mix", play_time_secs: 600, track_count: 3 }];

export const handlers: Handlers = {
  mix_list: () => mixes,
};
