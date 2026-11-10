// Mock of src-tauri/src/commands/export.rs. Add one entry per command, keyed by command name;
// this file is picked up automatically by devmock/index.ts.
import type { Handlers } from "./types";

export const handlers: Handlers = {
  export_render_preview: () => {
    throw "Mix preview needs the desktop app (audio rendering runs in Rust).";
  },
  export_mix_audio: (a) => ({ path: String(a.dest) }),
};
