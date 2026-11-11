// Mock of the native dialog plugin: a file pick returns three invented audio paths (one that
// is skipped, one that fails), a folder pick returns the fake drive. Shared across areas.
import type { Handlers } from "./types";

export const handlers: Handlers = {
  "plugin:dialog|open": (a) => {
    const o = a.options ?? {};
    if (o.multiple) return ["C:\\Music\\New Artist - New Tune.mp3", "C:\\Music\\Another - Skip Me.flac", "C:\\Music\\bad file.wav"];
    return "P:\\";
  },
  // a save dialog returns a fake path under a made-up folder, named after the suggested file
  "plugin:dialog|save": (a) => {
    const d = String(a.options?.defaultPath ?? "export");
    return `C:\\Exports\\${d.split(/[\\/]/).pop()}`;
  },
};
